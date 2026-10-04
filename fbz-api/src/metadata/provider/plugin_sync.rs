//! 插件元数据 provider 的同步执行路径（design §8 阶段 5 收尾）。
//!
//! 通过 [`PluginSyncInvoker`]（`plugins/invoke.rs`）把 `metadata.provider.query`
//! 事件同步下发给订阅插件，把 JSON 响应解析为受约束的
//! [`PluginMetadataContribution`]，交由 registry 按 §9 优先级合并。
//!
//! 失败哲学：插件永远不能拖死刮削任务。调用失败（超时/熔断/预算耗尽/非 JSON）
//! 一律降级为「无贡献」+ warn 日志，registry 记 NotMatched attempt 后继续。

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::warn;

use crate::plugins::invoke::{PluginSyncInvoker, PluginSyncSubscriber, SyncAuditMode};

use super::plugin::{PluginMetadataContribution, PluginMetadataProvider, PluginMetadataQuerier};
use super::shared::{
    MetadataArtwork, MetadataExternalId, MetadataLookup, MetadataMatch, MetadataNamedValue,
    MetadataPerson, MetadataProviderError, normalize_metadata_name,
};
use super::{MetadataProvider, ProviderContext};

/// 插件元数据查询的 hook 事件键（与 manifest `SUPPORTED_HOOK_EVENTS` 对齐）。
pub const METADATA_PROVIDER_QUERY_EVENT: &str = "metadata.provider.query";

/// 响应体各列表的上限：防御性截断，插件不能用超长列表撑爆刮削管线。
const MAX_EXTERNAL_IDS: usize = 32;
const MAX_ARTWORK: usize = 16;
const MAX_NAMED_VALUES: usize = 32;
const MAX_PEOPLE: usize = 100;
/// 允许插件写入的 artwork 类型（与 `artwork` 表 CHECK 对齐、排除 primary——
/// primary 语义由合并层控制）。
const ALLOWED_ARTWORK_TYPES: [&str; 5] = ["poster", "backdrop", "logo", "thumb", "banner"];
/// 人物职务 allowlist（与 `media_item_people.role_type` CHECK 对齐）。
/// 插件传入任意大小写/别名，这里规范化；未知职务降级为 actor，
/// 绝不能让插件数据打穿刮削任务的 DB 写入。
const ALLOWED_ROLE_TYPES: [&str; 7] = [
    "actor",
    "director",
    "writer",
    "producer",
    "composer",
    "artist",
    "guest_star",
];

/// 规范化人物职务到 DB allowlist；未知值降级为 `actor`。
fn normalize_role_type(value: Option<&str>) -> String {
    let normalized = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase().replace([' ', '-'], "_"))
        .unwrap_or_default();
    if ALLOWED_ROLE_TYPES.contains(&normalized.as_str()) {
        normalized
    } else {
        "actor".to_owned()
    }
}

/// 发现当前订阅 `metadata.provider.query` 的插件并包装为 provider 列表。
/// 每次刮削 job 调用一次（发现 SQL 走索引，成本可忽略），插件启停即时生效。
pub async fn discover_plugin_providers(
    invoker: &PluginSyncInvoker,
) -> Vec<Arc<dyn MetadataProvider>> {
    let subscribers = match invoker
        .list_subscribers(METADATA_PROVIDER_QUERY_EVENT)
        .await
    {
        Ok(subscribers) => subscribers,
        Err(err) => {
            warn!(error = %err, "failed to discover plugin metadata providers");
            return Vec::new();
        }
    };

    subscribers
        .into_iter()
        .map(|subscriber| {
            let plugin_id = subscriber.plugin_id.clone();
            let querier = Arc::new(SyncPluginQuerier {
                invoker: invoker.clone(),
                subscriber,
            });
            Arc::new(PluginMetadataProvider::new(&plugin_id, querier)) as Arc<dyn MetadataProvider>
        })
        .collect()
}

/// 绑定单个订阅插件的同步查询器。
pub struct SyncPluginQuerier {
    invoker: PluginSyncInvoker,
    subscriber: PluginSyncSubscriber,
}

#[async_trait]
impl PluginMetadataQuerier for SyncPluginQuerier {
    async fn query(
        &self,
        _ctx: &ProviderContext,
        input: &MetadataLookup,
        current: Option<&MetadataMatch>,
    ) -> Result<Option<PluginMetadataContribution>, MetadataProviderError> {
        let payload = lookup_payload(input, current);
        let outcome = self
            .invoker
            .invoke(
                &self.subscriber,
                METADATA_PROVIDER_QUERY_EVENT,
                &payload,
                SyncAuditMode::FailuresOnly,
            )
            .await;

        match outcome.result {
            Ok(value) => Ok(parse_plugin_metadata_response(&value)),
            Err(err) => {
                // 降级为「无贡献」：熔断/超时/预算耗尽都不是刮削失败。
                warn!(
                    plugin_id = %self.subscriber.plugin_id,
                    error = %err,
                    "plugin metadata provider query degraded to no contribution"
                );
                Ok(None)
            }
        }
    }
}

/// 下发给插件的查询载荷：识别层输出 + 当前已命中 match 的摘要（可空）。
/// 字段统一 camelCase，插件按需消费。
fn lookup_payload(input: &MetadataLookup, current: Option<&MetadataMatch>) -> Value {
    json!({
        "lookup": {
            "itemType": input.item_type,
            "title": input.title,
            "originalTitle": input.original_title,
            "productionYear": input.production_year,
            "season": input.season,
            "episode": input.episode,
            "tmdbId": input.tmdb_id,
            "imdbId": input.imdb_id,
            "tvdbId": input.tvdb_id,
            "language": input.language,
            "country": input.country,
        },
        "current": current.map(|found| json!({
            "provider": found.provider,
            "externalId": found.external_id,
            "title": found.title,
            "productionYear": found.production_year,
            "externalIds": found.external_ids.iter().map(|id| json!({
                "provider": id.provider,
                "externalId": id.external_id,
            })).collect::<Vec<_>>(),
        })),
    })
}

/// 插件响应的宽容形态：顶层或 `metadata` 字段下的贡献对象。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PluginMetadataResponseDto {
    metadata: Option<PluginContributionDto>,
    #[serde(flatten)]
    inline: PluginContributionDto,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct PluginContributionDto {
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    production_year: Option<i32>,
    premiere_date: Option<String>,
    official_rating: Option<String>,
    community_rating: Option<f32>,
    external_ids: Vec<PluginExternalIdDto>,
    artwork: Vec<PluginArtworkDto>,
    genres: Vec<NameOrObject>,
    studios: Vec<NameOrObject>,
    people: Vec<PluginPersonDto>,
}

impl PluginContributionDto {
    fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.original_title.is_none()
            && self.overview.is_none()
            && self.production_year.is_none()
            && self.premiere_date.is_none()
            && self.official_rating.is_none()
            && self.community_rating.is_none()
            && self.external_ids.is_empty()
            && self.artwork.is_empty()
            && self.genres.is_empty()
            && self.studios.is_empty()
            && self.people.is_empty()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginExternalIdDto {
    provider: String,
    external_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginArtworkDto {
    artwork_type: String,
    remote_url: String,
    #[serde(default)]
    is_primary: bool,
}

/// 分类值兼容两种形态：`"Drama"` 或 `{"name": "Drama"}`。
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NameOrObject {
    Plain(String),
    Object { name: String },
}

impl NameOrObject {
    fn into_name(self) -> String {
        match self {
            Self::Plain(name) => name,
            Self::Object { name } => name,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginPersonDto {
    name: String,
    #[serde(default)]
    role_type: Option<String>,
    #[serde(default)]
    role_name: Option<String>,
    #[serde(default)]
    profile_image_url: Option<String>,
}

/// 解析插件响应为受约束的贡献；非对象/空贡献返回 `None`。
/// 具体字段的长度/格式规范化由 `merge_plugin_metadata` 内的共享 normalizer 兜底，
/// 这里负责形态解析、列表截断和 artwork 类型 allowlist。
///
/// 兼容插件 SDK 的 `{ok, result}` 响应信封（`fbz-plugin-http.mjs` 的
/// `createHttpPluginServer` 会把 handler 返回值包进 `result`），插件作者可以
/// 用同一个 SDK server 同时处理异步 hook 派发和同步 provider 查询。
pub fn parse_plugin_metadata_response(value: &Value) -> Option<PluginMetadataContribution> {
    // SDK 信封解包：{ok: true, result: {...}} → result。
    let value = match value.get("result") {
        Some(result) if result.is_object() => result,
        _ => value,
    };
    let parsed: PluginMetadataResponseDto = serde_json::from_value(value.clone()).ok()?;
    let dto = match parsed.metadata {
        Some(nested) if !nested.is_empty() => nested,
        _ => parsed.inline,
    };
    if dto.is_empty() {
        return None;
    }

    let external_ids = dto
        .external_ids
        .into_iter()
        .take(MAX_EXTERNAL_IDS)
        .filter(|id| !id.provider.trim().is_empty() && !id.external_id.trim().is_empty())
        .map(|id| MetadataExternalId {
            provider: id.provider.trim().to_ascii_lowercase(),
            external_id: id.external_id.trim().to_owned(),
        })
        .collect();

    let artwork = dto
        .artwork
        .into_iter()
        .take(MAX_ARTWORK)
        .filter(|art| {
            ALLOWED_ARTWORK_TYPES.contains(&art.artwork_type.trim().to_ascii_lowercase().as_str())
        })
        .map(|art| MetadataArtwork {
            artwork_type: art.artwork_type.trim().to_ascii_lowercase(),
            source: None,
            remote_url: art.remote_url.trim().to_owned(),
            is_primary: art.is_primary,
        })
        .collect();

    let people = dto
        .people
        .into_iter()
        .take(MAX_PEOPLE)
        .filter(|person| !person.name.trim().is_empty())
        .enumerate()
        .map(|(index, person)| {
            let name = person.name.trim().to_owned();
            MetadataPerson {
                name_normalized: normalize_metadata_name(&name),
                name,
                role_type: normalize_role_type(person.role_type.as_deref()),
                role_name: person
                    .role_name
                    .map(|value| value.trim().to_owned())
                    .unwrap_or_default(),
                sort_order: index as i32,
                profile_image_url: person
                    .profile_image_url
                    .map(|value| value.trim().to_owned())
                    .filter(|value| !value.is_empty()),
            }
        })
        .collect();

    Some(PluginMetadataContribution {
        title: dto.title,
        original_title: dto.original_title,
        overview: dto.overview,
        production_year: dto.production_year,
        premiere_date: dto.premiere_date,
        official_rating: dto.official_rating,
        community_rating: dto.community_rating,
        external_ids,
        artwork,
        genres: named_values(dto.genres),
        studios: named_values(dto.studios),
        people,
    })
}

fn named_values(values: Vec<NameOrObject>) -> Vec<MetadataNamedValue> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .take(MAX_NAMED_VALUES)
        .map(NameOrObject::into_name)
        .filter_map(|name| {
            let name = name.trim().to_owned();
            if name.is_empty() {
                return None;
            }
            let name_normalized = normalize_metadata_name(&name);
            if !seen.insert(name_normalized.clone()) {
                return None;
            }
            Some(MetadataNamedValue {
                name,
                name_normalized,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_carries_lookup_and_current_match_summary() {
        let lookup = MetadataLookup {
            item_type: "movie".to_owned(),
            title: "Sintel".to_owned(),
            original_title: None,
            production_year: Some(2010),
            season: None,
            episode: None,
            tmdb_id: None,
            imdb_id: None,
            tvdb_id: None,
            language: Some("zh-CN".to_owned()),
            country: Some("CN".to_owned()),
            image_language: None,
            image_prefer_original: false,
            image_fallback_languages: Vec::new(),
        };

        let payload = lookup_payload(&lookup, None);

        assert_eq!(payload["lookup"]["title"], "Sintel");
        assert_eq!(payload["lookup"]["productionYear"], 2010);
        assert_eq!(payload["lookup"]["language"], "zh-CN");
        assert!(payload["current"].is_null());
    }

    #[test]
    fn parses_nested_and_inline_response_shapes() {
        let nested = json!({"metadata": {"overview": "from plugin"}});
        let inline = json!({"overview": "inline"});
        let sdk_envelope = json!({"ok": true, "result": {"metadata": {"overview": "sdk"}}});

        assert_eq!(
            parse_plugin_metadata_response(&nested).unwrap().overview,
            Some("from plugin".to_owned())
        );
        assert_eq!(
            parse_plugin_metadata_response(&inline).unwrap().overview,
            Some("inline".to_owned())
        );
        assert_eq!(
            parse_plugin_metadata_response(&sdk_envelope)
                .unwrap()
                .overview,
            Some("sdk".to_owned())
        );
    }

    #[test]
    fn empty_or_invalid_responses_yield_none() {
        assert!(parse_plugin_metadata_response(&json!({})).is_none());
        assert!(parse_plugin_metadata_response(&json!({"metadata": {}})).is_none());
        assert!(parse_plugin_metadata_response(&json!("just a string")).is_none());
        assert!(parse_plugin_metadata_response(&json!(42)).is_none());
    }

    #[test]
    fn artwork_types_are_allowlisted_and_lists_capped() {
        let response = json!({
            "artwork": [
                {"artworkType": "poster", "remoteUrl": "https://img.test/a.jpg"},
                {"artworkType": "primary", "remoteUrl": "https://img.test/b.jpg"},
                {"artworkType": "nonsense", "remoteUrl": "https://img.test/c.jpg"}
            ],
            "genres": ["Drama", "Drama", {"name": "Sci-Fi"}, ""],
        });

        let contribution = parse_plugin_metadata_response(&response).unwrap();
        assert_eq!(contribution.artwork.len(), 1);
        assert_eq!(contribution.artwork[0].artwork_type, "poster");
        // 去重 + 丢弃空串：Drama、Sci-Fi。
        assert_eq!(contribution.genres.len(), 2);
    }

    #[test]
    fn external_ids_normalize_provider_case() {
        let response = json!({
            "externalIds": [
                {"provider": "IMDB", "externalId": " tt0123 "},
                {"provider": " ", "externalId": "x"}
            ]
        });

        let contribution = parse_plugin_metadata_response(&response).unwrap();
        assert_eq!(contribution.external_ids.len(), 1);
        assert_eq!(contribution.external_ids[0].provider, "imdb");
        assert_eq!(contribution.external_ids[0].external_id, "tt0123");
    }

    #[test]
    fn people_roles_normalize_to_db_allowlist() {
        let response = json!({
            "people": [
                {"name": "Alice", "roleType": "Director"},
                {"name": "Bob"},
                {"name": "Carol", "roleType": "Guest Star"},
                {"name": "Dave", "roleType": "Showrunner"},
                {"name": "  "}
            ]
        });

        let contribution = parse_plugin_metadata_response(&response).unwrap();
        assert_eq!(contribution.people.len(), 4);
        // 大小写规范化、别名映射、未知职务降级 actor —— 与
        // media_item_people.role_type CHECK 对齐，插件数据不能打穿 DB 写入。
        assert_eq!(contribution.people[0].role_type, "director");
        assert_eq!(contribution.people[1].role_type, "actor");
        assert_eq!(contribution.people[2].role_type, "guest_star");
        assert_eq!(contribution.people[3].role_type, "actor");
        assert_eq!(contribution.people[1].sort_order, 1);
    }
}

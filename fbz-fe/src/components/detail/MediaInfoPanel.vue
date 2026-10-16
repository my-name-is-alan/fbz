<script setup lang="ts">
import type { MediaSourceInfo } from "@/service/modules/mediaInfo.ts";
import {
  formatBitrate,
  formatBytes,
  formatClock,
  formatSampleRate,
  sourceLabel,
} from "@/service/modules/mediaInfo.ts";

/**
 * 「媒体信息」面板（JF / Emby 级 A/V 元数据）。
 * 数据来自 PlaybackInfo 的 MediaSources / MediaStreams 真字段；缺失字段整行隐藏。
 * 多版本时可在面板内切换版本（与详情页版本选择双向同步）。
 */
interface Props {
  sources: MediaSourceInfo[];
  /** 章节 / 片头片尾标记数量。 */
  chapterCount?: number;
  /** 紧凑模式（弹窗内）：不显示外层标题。 */
  compact?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  chapterCount: 0,
  compact: false,
});

const activeId = defineModel<string>("activeId", { default: "" });

const active = computed(
  () => props.sources.find((s) => s.id === activeId.value) ?? props.sources[0],
);

function langLabel(code: string | undefined): string {
  if (!code) return "未知";
  const map: Record<string, string> = {
    chi: "中文",
    zho: "中文",
    zh: "中文",
    chs: "简体中文",
    cht: "繁体中文",
    eng: "英语",
    en: "英语",
    jpn: "日语",
    ja: "日语",
    kor: "韩语",
    ko: "韩语",
    fre: "法语",
    fra: "法语",
    ger: "德语",
    deu: "德语",
    spa: "西班牙语",
    rus: "俄语",
    und: "未定义",
  };
  return map[code.toLowerCase()] ?? code;
}

const summary = computed(() => {
  const s = active.value;
  if (!s) return [];
  return [
    { icon: "file", label: "容器", value: s.container?.toUpperCase() },
    { icon: "runtime", label: "时长", value: formatClock(s.runtimeSeconds) },
    { icon: "size", label: "大小", value: formatBytes(s.size) },
    { icon: "bitrate", label: "总码率", value: formatBitrate(s.bitrate) },
    {
      icon: "hash",
      label: "章节",
      value: props.chapterCount ? `${props.chapterCount} 个` : undefined,
    },
  ].filter((row) => row.value);
});

const capabilities = computed(() => {
  const s = active.value;
  if (!s) return [];
  return [
    { label: "直接播放", on: s.supportsDirectPlay },
    { label: "直接串流", on: s.supportsDirectStream },
    { label: "转码", on: s.supportsTranscoding },
  ].filter((c) => c.on != null);
});
</script>

<template>
  <section v-if="active" class="media-info" :class="{ compact: props.compact }">
    <header v-if="!props.compact" class="mi-head">
      <h2 class="c-section-title"><BaseIcon name="info" :size="16" />媒体信息</h2>
      <span v-if="active.isRemote" class="flag">
        <BaseIcon name="cloud" :size="12" />远程 / STRM
      </span>
    </header>

    <div v-if="props.sources.length > 1" class="versions" role="tablist" aria-label="版本">
      <button
        v-for="(src, i) in props.sources"
        :key="src.id"
        type="button"
        role="tab"
        class="ver"
        :class="{ on: src.id === active.id }"
        :aria-selected="src.id === active.id"
        @click="activeId = src.id"
      >
        <BaseIcon name="versions" :size="13" />{{ sourceLabel(src, i) }}
      </button>
    </div>

    <dl v-if="summary.length" class="summary">
      <div v-for="row in summary" :key="row.label" class="sum">
        <dt><BaseIcon :name="row.icon" :size="13" />{{ row.label }}</dt>
        <dd>{{ row.value }}</dd>
      </div>
      <div v-if="capabilities.length" class="sum caps">
        <dt><BaseIcon name="play" :size="13" />播放能力</dt>
        <dd>
          <span v-for="cap in capabilities" :key="cap.label" class="cap" :class="{ off: !cap.on }">
            <BaseIcon :name="cap.on ? 'check' : 'close'" :size="11" :stroke-width="2.4" />{{
              cap.label
            }}
          </span>
        </dd>
      </div>
    </dl>

    <div class="streams">
      <article v-for="(v, i) in active.video" :key="`v-${v.index ?? i}`" class="stream-card">
        <h3>
          <BaseIcon name="resolution" :size="14" />视频<span v-if="active.video.length > 1">
            {{ i + 1 }}</span
          >
        </h3>
        <div class="tags">
          <span v-if="v.resolution" class="tag strong">{{ v.resolution }}</span>
          <span v-if="v.dynamicRange" class="tag accent">{{ v.dynamicRange }}</span>
          <span v-if="v.codec" class="tag">{{ v.codec }}</span>
          <span v-if="v.bitDepth" class="tag">{{ v.bitDepth }}bit</span>
        </div>
        <dl class="kv">
          <div v-if="v.width && v.height">
            <dt>分辨率</dt>
            <dd>{{ v.width }} × {{ v.height }}</dd>
          </div>
          <div v-if="v.profile || v.level != null">
            <dt>档次 / 级别</dt>
            <dd>
              {{ [v.profile, v.level != null ? `L${v.level}` : ""].filter(Boolean).join(" · ") }}
            </dd>
          </div>
          <div v-if="v.bitRate">
            <dt>码率</dt>
            <dd>{{ formatBitrate(v.bitRate) }}</dd>
          </div>
          <div v-if="v.title">
            <dt>标题</dt>
            <dd>{{ v.title }}</dd>
          </div>
        </dl>
      </article>
    </div>

    <div v-if="active.audio.length" class="table-block">
      <h3>
        <BaseIcon name="audio" :size="14" />音轨 <span class="n">{{ active.audio.length }}</span>
      </h3>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>语言</th>
              <th>编码</th>
              <th>声道</th>
              <th>采样率</th>
              <th>码率</th>
              <th>标记</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(a, i) in active.audio" :key="`a-${a.index ?? i}`">
              <td class="mono">{{ a.index ?? i }}</td>
              <td>
                {{ langLabel(a.language) }}<small v-if="a.title"> · {{ a.title }}</small>
              </td>
              <td class="mono">
                {{ a.codec ?? "—" }}<small v-if="a.profile"> {{ a.profile }}</small>
              </td>
              <td class="mono">{{ a.layout ?? "—" }}</td>
              <td class="mono">{{ formatSampleRate(a.sampleRate) ?? "—" }}</td>
              <td class="mono">{{ formatBitrate(a.bitRate) ?? "—" }}</td>
              <td class="flags">
                <span v-if="a.isDefault" class="mini">默认</span>
                <span v-if="a.isForced" class="mini">强制</span>
                <span v-if="a.isExternal" class="mini ext">外挂</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>

    <div v-if="active.subtitles.length" class="table-block">
      <h3>
        <BaseIcon name="subtitles" :size="14" />字幕
        <span class="n">{{ active.subtitles.length }}</span>
      </h3>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>#</th>
              <th>语言</th>
              <th>格式</th>
              <th>标题</th>
              <th>来源</th>
              <th>标记</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(s, i) in active.subtitles" :key="`s-${s.index ?? i}`">
              <td class="mono">{{ s.index ?? i }}</td>
              <td>{{ langLabel(s.language) }}</td>
              <td class="mono">{{ s.codec ?? "—" }}</td>
              <td>{{ s.title ?? "—" }}</td>
              <td>
                <span class="mini" :class="{ ext: s.isExternal }">
                  <BaseIcon
                    :name="s.isExternal ? 'external' : 'file'"
                    :size="10"
                    :stroke-width="2.2"
                  />
                  {{ s.isExternal ? "外挂" : "内嵌" }}
                </span>
              </td>
              <td class="flags">
                <span v-if="s.isDefault" class="mini">默认</span>
                <span v-if="s.isForced" class="mini">强制</span>
              </td>
            </tr>
          </tbody>
        </table>
      </div>
    </div>
    <p v-else-if="!props.compact" class="c-meta no-subs">
      <BaseIcon name="subtitles" :size="13" />此版本没有字幕轨，可通过「外挂字幕」搜索。
    </p>
  </section>
</template>

<style scoped lang="scss">
.media-info {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 18px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-lg);
  background: var(--ui-surface);

  &.compact {
    padding: 0;
    border: 0;
    background: transparent;
  }
}

.mi-head {
  display: flex;
  align-items: center;
  justify-content: space-between;

  :deep(.fbz-icon) {
    color: var(--ui-text-subtle);
  }
}

.flag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 5px;
  color: var(--ui-text-muted);
  font-size: 11.5px;
}

.versions {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.ver {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  max-width: 100%;
  height: 28px;
  padding: 0 10px;
  overflow: hidden;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-sm);
  background: var(--ui-surface-raised);
  color: var(--ui-text-muted);
  font-size: 12px;
  white-space: nowrap;
  text-overflow: ellipsis;
  cursor: pointer;

  &:hover {
    color: var(--ui-text);
  }

  &.on {
    border-color: var(--ui-accent);
    color: var(--ui-accent-hover);
    background: rgb(var(--ui-accent-channel) / 10%);
  }
}

.summary {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: 8px;
  margin: 0;
}

.sum {
  padding: 10px 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);

  dt {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-bottom: 4px;
    color: var(--ui-text-subtle);
    font-size: 11.5px;
  }

  dd {
    margin: 0;
    color: var(--ui-text);
    font-family: var(--ui-font-mono);
    font-size: 13px;
    font-weight: 500;
  }

  &.caps {
    grid-column: span 2;

    dd {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
      font-family: var(--ui-font-sans);
    }
  }
}

.cap {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  color: var(--ui-success);
  font-size: 12px;

  &.off {
    color: var(--ui-text-subtle);
  }
}

.streams {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 10px;

  &:empty {
    display: none;
  }
}

.stream-card {
  padding: 12px;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
  background: var(--ui-surface-raised);
}

h3 {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 0 8px;
  color: var(--ui-text);
  font-size: 13px;
  font-weight: 600;

  :deep(.fbz-icon) {
    color: var(--ui-text-subtle);
  }

  .n {
    color: var(--ui-text-subtle);
    font-weight: 500;
  }
}

.tags {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
  margin-bottom: 8px;
}

.tag {
  height: 20px;
  padding: 0 6px;
  display: inline-flex;
  align-items: center;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 11px;
  font-weight: 700;

  &.strong {
    color: var(--ui-text);
  }

  &.accent {
    border-color: rgb(var(--ui-accent-channel) / 50%);
    color: var(--ui-accent-hover);
  }
}

.kv {
  display: grid;
  gap: 4px;
  margin: 0;

  div {
    display: flex;
    gap: 10px;
    font-size: 12.5px;
  }

  dt {
    flex: 0 0 76px;
    color: var(--ui-text-subtle);
  }

  dd {
    margin: 0;
    color: var(--ui-text);
    font-family: var(--ui-font-mono);
    word-break: break-all;
  }
}

.table-wrap {
  overflow-x: auto;
  border: 1px solid var(--ui-border);
  border-radius: var(--ui-radius-md);
}

table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}

th,
td {
  padding: 7px 10px;
  border-bottom: 1px solid var(--ui-border);
  text-align: left;
  white-space: nowrap;
}

th {
  background: var(--ui-surface-raised);
  color: var(--ui-text-subtle);
  font-size: 11.5px;
  font-weight: 600;
}

tbody tr:last-child td {
  border-bottom: 0;
}

tbody tr:hover {
  background: var(--ui-surface-hover);
}

td {
  color: var(--ui-text);

  small {
    color: var(--ui-text-subtle);
  }
}

.mono {
  font-family: var(--ui-font-mono);
}

.flags {
  display: flex;
  gap: 4px;
}

.mini {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 18px;
  padding: 0 5px;
  border: 1px solid var(--ui-border-strong);
  border-radius: 4px;
  color: var(--ui-text-muted);
  font-size: 10.5px;

  &.ext {
    border-color: rgb(var(--ui-accent-channel) / 50%);
    color: var(--ui-accent-hover);
  }
}

.no-subs {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0;
}

@media (max-width: 600px) {
  .media-info:not(.compact) {
    padding: 12px;
  }

  .sum.caps {
    grid-column: auto;
  }
}
</style>

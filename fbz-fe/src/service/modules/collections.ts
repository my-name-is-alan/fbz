/**
 * 合集（BoxSet）C 端：对接
 * - `GET /Users/{userId}/Items?IncludeItemTypes=BoxSet&Recursive=true`
 * - `POST /Collections?Name=&Ids=`
 * - `POST /Collections/{id}/Items?Ids=`
 *
 * 详情仍走 `detail.ts` 的 `/Collections/{id}` 与 `/Items`。
 * 写操作后端限管理员（`can_manage_server`）。
 */
import { embyRequest, mediaImageUrl } from "@/service/request.ts";
import type { MediaItem } from "@/types/media.ts";

interface BaseItemDto {
  Id: string;
  Name: string;
  Type: string;
  ProductionYear?: number | null;
  CommunityRating?: number | null;
  ChildCount?: number | null;
  ImageTags?: Record<string, string> | null;
  Overview?: string | null;
}

interface ItemsResultDto {
  Items?: BaseItemDto[];
  TotalRecordCount?: number;
  StartIndex?: number;
}

interface CollectionWriteResultDto {
  Id: string;
  Name: string;
}

export interface CollectionsPage {
  items: MediaItem[];
  total: number;
}

export interface CollectionSummary {
  id: string;
  name: string;
}

/** 列出用户可见合集（BoxSet）。 */
export async function listCollections(
  userId: string,
  opts?: { startIndex?: number; limit?: number },
): Promise<CollectionsPage> {
  const { data } = await embyRequest.get<ItemsResultDto>(
    `/Users/${encodeURIComponent(userId)}/Items`,
    {
      params: {
        IncludeItemTypes: "BoxSet",
        Recursive: true,
        EnableImages: true,
        Fields: "ChildCount,Overview,PrimaryImageAspectRatio",
        SortBy: "SortName",
        SortOrder: "Ascending",
        StartIndex: opts?.startIndex ?? 0,
        Limit: opts?.limit ?? 100,
      },
    },
  );
  const dtos = (data.Items ?? []).filter(
    (item) => item.Type === "BoxSet" || item.Type === "Collection",
  );
  return {
    items: dtos.map((dto) => {
      const count = dto.ChildCount;
      return {
        id: dto.Id,
        libraryId: "collection",
        title: dto.Name,
        meta:
          count != null
            ? `${count} 部作品`
            : [dto.ProductionYear, "合集"].filter(Boolean).join(" · "),
        detailType: "collection",
        poster: dto.ImageTags?.Primary
          ? mediaImageUrl(`/Items/${dto.Id}/Images/Primary`)
          : undefined,
        year: dto.ProductionYear ?? undefined,
        rating: dto.CommunityRating ?? undefined,
      } satisfies MediaItem;
    }),
    total: data.TotalRecordCount ?? dtos.length,
  };
}

/** 右键「加入合集」用的轻量列表。 */
export async function listCollectionSummaries(userId: string): Promise<CollectionSummary[]> {
  const page = await listCollections(userId, { limit: 100 });
  return page.items.map((item) => ({ id: String(item.id), name: item.title }));
}

/** 创建合集（可选初始成员 Ids）。管理员。 */
export async function createCollection(opts: {
  name: string;
  ids?: string[];
}): Promise<CollectionSummary> {
  const { data } = await embyRequest.post<CollectionWriteResultDto>("/Collections", null, {
    params: {
      Name: opts.name.trim(),
      Ids: opts.ids?.length ? opts.ids.join(",") : undefined,
    },
  });
  return { id: data.Id, name: data.Name };
}

/** 向合集追加媒体条目。管理员；204 无 body。 */
export async function addCollectionItems(collectionId: string, itemIds: string[]): Promise<void> {
  const ids = itemIds.map((id) => id.trim()).filter(Boolean);
  if (!ids.length) return;
  await embyRequest.post(`/Collections/${encodeURIComponent(collectionId)}/Items`, null, {
    params: { Ids: ids.join(",") },
  });
}

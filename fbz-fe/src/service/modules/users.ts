import { request } from "@/service/request.ts";

/**
 * 用户头像上传/删除。头像 GET 走无鉴权的 `<img src>`（见 `request.ts:userAvatarUrl`），
 * 这里只封装需要令牌的写操作。上传体为原始图片字节，后端按魔数识别类型。
 */

/** 上传/替换指定用户头像。file 为浏览器 File/Blob，原样作为请求体。 */
export async function uploadUserAvatar(userId: string, file: Blob): Promise<void> {
  await request.post(`/users/${encodeURIComponent(userId)}/avatar`, file, {
    headers: { "Content-Type": file.type || "application/octet-stream" },
  });
}

/** 删除指定用户头像，恢复首字母头像。 */
export async function deleteUserAvatar(userId: string): Promise<void> {
  await request.delete(`/users/${encodeURIComponent(userId)}/avatar`);
}

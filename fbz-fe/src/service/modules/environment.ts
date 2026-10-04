import { embyRequest } from "@/service/request.ts";
import type {
  DefaultDirectoryBrowserInfo,
  DirectoryContentsQuery,
  FileSystemEntryInfo,
  ValidatePathRequest,
} from "@/types/environment.ts";

/** 读取后端当前默认浏览目录。 */
export async function getDefaultDirectoryBrowser(): Promise<DefaultDirectoryBrowserInfo> {
  const { data } = await embyRequest.get<DefaultDirectoryBrowserInfo>(
    "/Environment/DefaultDirectoryBrowser",
  );
  return data;
}

/** 列出 Rust 后端所在服务器可见的磁盘/根目录。 */
export async function listDrives(): Promise<FileSystemEntryInfo[]> {
  const { data } = await embyRequest.get<FileSystemEntryInfo[]>("/Environment/Drives");
  return data;
}

/** 列出指定目录内容。默认只列目录，避免媒体文件列表过大。 */
export async function listDirectoryContents(
  query: DirectoryContentsQuery,
): Promise<FileSystemEntryInfo[]> {
  const { data } = await embyRequest.get<FileSystemEntryInfo[]>("/Environment/DirectoryContents", {
    params: {
      path: query.path,
      includeFiles: query.includeFiles ?? false,
      includeDirectories: query.includeDirectories ?? true,
    },
  });
  return data;
}

/** 获取父目录路径。 */
export async function getParentPath(path: string): Promise<string> {
  const { data } = await embyRequest.get<string>("/Environment/ParentPath", {
    params: { path },
  });
  return data;
}

/** 校验目录是否存在且类型匹配。 */
export async function validatePath(
  path: string,
  payload: ValidatePathRequest = { isFile: false },
): Promise<void> {
  await embyRequest.post("/Environment/ValidatePath", payload, { params: { path } });
}

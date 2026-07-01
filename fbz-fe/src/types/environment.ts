export type FileSystemEntryType = "File" | "Directory" | "NetworkComputer" | "NetworkShare";

export interface DefaultDirectoryBrowserInfo {
  Path: string;
}

export interface FileSystemEntryInfo {
  Name: string;
  Path: string;
  Type: FileSystemEntryType;
}

export interface DirectoryContentsQuery {
  path: string;
  includeFiles?: boolean;
  includeDirectories?: boolean;
}

export interface ValidatePathRequest {
  validateWriteable?: boolean;
  isFile?: boolean;
  username?: string;
  password?: string;
}

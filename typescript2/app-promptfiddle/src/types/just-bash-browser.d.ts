declare module "just-bash/browser" {
  export interface FsStat {
    isFile: boolean;
    isDirectory: boolean;
    isSymbolicLink: boolean;
    mode: number;
    size: number;
    mtime?: Date;
    ctime?: Date;
    atime?: Date;
  }

  export interface MkdirOptions {
    recursive?: boolean;
  }

  export interface RmOptions {
    recursive?: boolean;
    force?: boolean;
  }

  export interface CpOptions {
    recursive?: boolean;
    force?: boolean;
  }

  export interface IFileSystem {
    readFile(path: string): Promise<string>;
    readFileBuffer(path: string): Promise<Uint8Array>;
    writeFile(path: string, content: string | Uint8Array): Promise<void>;
    appendFile(path: string, content: string | Uint8Array): Promise<void>;
    exists(path: string): Promise<boolean>;
    stat(path: string): Promise<FsStat>;
    readdir(path: string): Promise<string[]>;
    mkdir(path: string, options?: MkdirOptions): Promise<void>;
    rm(path: string, options?: RmOptions): Promise<void>;
    cp(src: string, dest: string, options?: CpOptions): Promise<void>;
    mv(src: string, dest: string): Promise<void>;
  }

  export interface BashExecOptions {
    cwd?: string;
    env?: Record<string, string>;
    stdin?: string;
    replaceEnv?: boolean;
    signal?: AbortSignal;
  }

  export interface BashExecResult {
    stdout: string;
    stderr: string;
    exitCode: number;
  }

  export class InMemoryFs implements IFileSystem {
    constructor();
    readFile(path: string): Promise<string>;
    readFileBuffer(path: string): Promise<Uint8Array>;
    writeFile(path: string, content: string | Uint8Array): Promise<void>;
    appendFile(path: string, content: string | Uint8Array): Promise<void>;
    exists(path: string): Promise<boolean>;
    stat(path: string): Promise<FsStat>;
    readdir(path: string): Promise<string[]>;
    mkdir(path: string, options?: MkdirOptions): Promise<void>;
    rm(path: string, options?: RmOptions): Promise<void>;
    cp(src: string, dest: string, options?: CpOptions): Promise<void>;
    mv(src: string, dest: string): Promise<void>;
  }

  export class MountableFs implements IFileSystem {
    constructor(options: { base: IFileSystem });
    mount(path: string, fs: IFileSystem): void;
    readFile(path: string): Promise<string>;
    readFileBuffer(path: string): Promise<Uint8Array>;
    writeFile(path: string, content: string | Uint8Array): Promise<void>;
    appendFile(path: string, content: string | Uint8Array): Promise<void>;
    exists(path: string): Promise<boolean>;
    stat(path: string): Promise<FsStat>;
    readdir(path: string): Promise<string[]>;
    mkdir(path: string, options?: MkdirOptions): Promise<void>;
    rm(path: string, options?: RmOptions): Promise<void>;
    cp(src: string, dest: string, options?: CpOptions): Promise<void>;
    mv(src: string, dest: string): Promise<void>;
  }

  export class Bash {
    constructor(options: { fs: IFileSystem; cwd: string });
    exec(command: string, options?: BashExecOptions): Promise<BashExecResult>;
  }
}

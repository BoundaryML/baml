export interface LoadResult {
    path?: string;
    config: any;
}
export declare function resolve(cwd: string, filename?: string): Promise<string | void>;
export declare function resolveSync(cwd: string, filename?: string): string | void;
export declare function find(dir: string): Promise<string | void>;
export declare function findSync(dir: string): string | void;
export declare function load(cwd: string, filename?: string): Promise<LoadResult>;
export declare function loadSync(cwd: string, filename?: string): LoadResult;
export declare function readFile(filename: string): Promise<any>;
export declare function readFileSync(filename: string): any;
export declare function parse(contents: string, filename: string): any;

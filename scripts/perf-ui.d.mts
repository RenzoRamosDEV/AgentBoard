export interface ChunkSize {
  file: string;
  bytes: number;
  gzip: number;
}

export function chunkSizes(dir: string): ChunkSize[];

export function initialChunks(indexHtml: string): Set<string>;

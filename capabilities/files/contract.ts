/**
 * Structural contract for the Files capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface FilesApi {
  write(bytes: Uint8Array, options?: { extension?: string }): Promise<string>;
  read(reference: string): Promise<Uint8Array>;
  url(reference: string): Promise<string>;
  delete(reference: string): Promise<void>;
}

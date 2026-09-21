/**
 * Structural contract for the Http capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface HttpResponse {
  status: number;
  ok: boolean;
  body: string;
}

export interface HttpApi {
  get(url: string): Promise<HttpResponse>;
}

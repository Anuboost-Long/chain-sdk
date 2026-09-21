/**
 * Structural contract for the AgentServer capability.
 * See CONTRACT.md for the semantic contract this type shape must satisfy.
 */

export interface AgentServerRequest {
  method: string;
  path: string;
  headers: Record<string, string>;
  body: string;
}

export interface AgentServerResponse {
  status: number;
  headers?: Record<string, string>;
  body: string;
}

export type AgentServerHandler = (request: AgentServerRequest) => Promise<AgentServerResponse>;

export interface AgentServerInfo {
  port: number;
}

export interface AgentServerApi {
  start(handler: AgentServerHandler, options?: { port?: number }): Promise<AgentServerInfo>;
  stop(): Promise<void>;
}

/* eslint-disable */
// Generated barrel. DO NOT EDIT.

export * from "./client";
export * from "./errors";
export * from "./auth";
export * from "./retry";
export * from "./paginate";

export * from "./models/Pet";
export * from "./models/Pets";
export * from "./models/Error";

import { PetsApi } from "./api/Pets";

import { ApiClient } from "./client";
import type { ApiClientOptions } from "./client";

export { PetsApi } from "./api/Pets";

export interface SdkClient {
  pets: PetsApi;
}

/**
 * Construct the SDK client. Each tag becomes a namespaced property.
 */
export function createClient(options: ApiClientOptions = {}): SdkClient {
  const client = new ApiClient(options);
  return {
    pets: new PetsApi(client),
  };
}

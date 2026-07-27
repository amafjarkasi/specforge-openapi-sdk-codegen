/* eslint-disable */
// Generated barrel. DO NOT EDIT.

// Tree-shaking: this file exports only the client core, runtime helpers,
// and models. API tag classes are intentionally excluded so bundlers can
// eliminate unused tags. Import individual tag modules directly:
//
//   import { PetsApi } from "./api/Pets";
//   import type { ListPetsParams } from "./api/Pets";
//
// Or use the convenience barrel that re-exports all tags:
//
//   import { PetsApi } from "./api";
//
// The createClient() factory below pulls all tags into a single object.
// Prefer direct tag imports when bundle size matters.

export * from "./client";
export * from "./interceptors";
export * from "./errors";
export * from "./auth";
export * from "./retry";
export * from "./paginate";
export * from "./validate";
export * from "./ratelimit";
export * from "./telemetry";
export * from "./logging";

export * from "./models/Error";
export * from "./models/Pet";
export * from "./models/Pets";

import { PetsApi } from "./api/Pets";

import { ApiClient } from "./client";
import type { ApiClientOptions } from "./client";

export interface SdkClient {
  pets: PetsApi;
}

/**
 * Construct the SDK client. Each tag becomes a namespaced property.
 *
 * NOTE: This factory imports all tag modules. For tree-shakeable code,
 * import individual tag modules directly instead of using createClient.
 */
export function createClient(options: ApiClientOptions = {}): SdkClient {
  const client = new ApiClient(options);
  return {
    pets: new PetsApi(client),
  };
}

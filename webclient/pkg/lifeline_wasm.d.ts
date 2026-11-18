/* tslint:disable */
/* eslint-disable */

/**
 * This identity's stable address — the routing tag others send to.
 */
export function address(backup_json: string, passphrase: string): string;

/**
 * Mint a new identity; returns its passphrase-wrapped backup as JSON for the
 * browser to store (IndexedDB). `now` is unix seconds from JS — WASM has no clock.
 */
export function new_identity(now: bigint, passphrase: string, display_name?: string | null): string;

/**
 * Open a ciphertext bundle addressed to us; returns the plaintext. Errors if it
 * isn't for us or the sender signature doesn't verify (anti-impersonation).
 */
export function open(backup_json: string, passphrase: string, bundle_cbor: Uint8Array): string;

/**
 * Your public identity record (CBOR). Share it (QR / pairing code) so others can
 * seal messages to you.
 */
export function public_record(backup_json: string, passphrase: string): Uint8Array;

/**
 * The routing address inside a contact's public record (the tag to send to).
 */
export function record_address(record: Uint8Array): string;

/**
 * Seal a text message to a recipient's public record. Returns the opaque
 * ciphertext bundle (CBOR) for the transport to carry; the server never sees the text.
 */
export function seal(backup_json: string, passphrase: string, recipient_record: Uint8Array, now: bigint, text: string): Uint8Array;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly address: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly new_identity: (a: bigint, b: number, c: number, d: number, e: number) => [number, number, number, number];
    readonly open: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly public_record: (a: number, b: number, c: number, d: number) => [number, number, number, number];
    readonly record_address: (a: number, b: number) => [number, number, number, number];
    readonly seal: (a: number, b: number, c: number, d: number, e: number, f: number, g: bigint, h: number, i: number) => [number, number, number, number];
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;

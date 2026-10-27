/**
 * NobodyWho — React Native bindings for local LLM inference.
 */

// Native crate install + bindings initialize. These run as side effects of
// evaluating this module, which the React Native runtime is guaranteed to do
// before any FFI call because this file is the package main.
//
// The install logic was previously reached only through lazy re-exports from
// ./index.tsx. In release mode Metro's inlineRequires defers lazy re-exports
// until first use, so consumers whose first interaction was Model.load /
// downloadModel / Chat never triggered ./index and FFI calls then threw
// "Cannot read property 'ubrn_…' of undefined". Inlining the install here
// removes that indirection so this can't recur.
import installer from "./NativeNobodywho";
import nobodywho from "./generated";
// Register the rust crate with Hermes
installer.installRustCrate();
// Initialize the generated bindings: mostly checksums, but also callbacks.
nobodywho.initialize();

// Re-export the wrappers.
export * from "./wrapper";

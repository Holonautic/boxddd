#![allow(clippy::approx_constant)]
#![allow(clippy::doc_lazy_continuation)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::ptr_offset_with_cast)]
#![allow(clippy::suspicious_doc_comments)]
#![allow(clippy::tabs_in_doc_comments)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::unreadable_literal)]
#![allow(clippy::useless_transmute)]
#![allow(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]

#[cfg(boxddd_sys_wasm_provider)]
include!(concat!(env!("OUT_DIR"), "/wasm_provider_bindings.rs"));

#[cfg(all(
    not(boxddd_sys_wasm_provider),
    feature = "double-precision",
    has_pregenerated,
    not(force_bindgen)
))]
include!("bindings_pregenerated_double.rs");

#[cfg(all(
    not(boxddd_sys_wasm_provider),
    not(feature = "double-precision"),
    has_pregenerated,
    not(force_bindgen)
))]
include!("bindings_pregenerated.rs");

#[cfg(all(
    not(boxddd_sys_wasm_provider),
    any(force_bindgen, not(has_pregenerated))
))]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

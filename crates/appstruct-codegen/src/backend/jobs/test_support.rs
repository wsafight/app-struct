use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn test_worker_gate_wait(poll_interval_ms: u64) -> TokenStream {
    quote! {
        if !test_worker_gate_open() {
            tokio::select! {
                () = tokio::time::sleep(Duration::from_millis(#poll_interval_ms)) => {}
                result = lane_receiver.changed() => if result.is_err() { break; }
            }
            continue;
        }
    }
}

pub(super) fn test_worker_gate_source() -> TokenStream {
    quote! {
        #[cfg(feature = "test-support")]
        fn test_worker_gate_open() -> bool {
            env::var_os("APPSTRUCT_TEST_JOB_GATE")
                .is_none_or(|path| std::path::Path::new(&path).is_file())
        }
        #[cfg(not(feature = "test-support"))]
        fn test_worker_gate_open() -> bool { true }
    }
}

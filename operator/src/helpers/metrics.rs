use prometheus::{register_histogram_vec, register_int_counter, HistogramVec, IntCounter};

/// prometheus metrics exposed on /metrics
#[derive(Clone)]
pub struct Metrics {
    pub set_handled_events: IntCounter,
    pub proxy_handled_events: IntCounter,
    pub set_reconcile_duration: HistogramVec,
    pub proxy_reconcile_duration: HistogramVec,
}

impl Metrics {
    pub(crate) fn new() -> Self {
        let set_reconcile_histogram = register_histogram_vec!(
            "mcset_controller_reconcile_duration_seconds",
            "The duration of mcset reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();

        let proxy_reconcile_histogram = register_histogram_vec!(
            "mcproxy_controller_reconcile_duration_seconds",
            "The duration of mcproxy reconcile to complete in seconds",
            &[],
            vec![0.01, 0.1, 0.25, 0.5, 1., 5., 15., 60.]
        )
        .unwrap();

        Metrics {
            set_handled_events: register_int_counter!(
                "mcset_controller_handled_events",
                "mcset handled events"
            )
            .unwrap(),
            proxy_handled_events: register_int_counter!(
                "proxy_controller_handled_events",
                "proxy handled events"
            )
            .unwrap(),
            set_reconcile_duration: set_reconcile_histogram,
            proxy_reconcile_duration: proxy_reconcile_histogram,
        }
    }
}

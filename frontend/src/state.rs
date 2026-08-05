//! Global application state management

use crate::types::*;
use leptos::prelude::*;
use std::sync::Arc;
use tokio::sync::RwLock as AsyncRwLock;

#[derive(Clone)]
pub struct AppState {
    pdsg_snapshot: RwSignal<Option<PdsgSnapshot>>,
    alcoa_report: RwSignal<Option<AlcoaReport>>,
    sensor_readings: RwSignal<Vec<SensorReading>>,
    selected_node: RwSignal<Option<String>>,
    loading: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    current_actor: RwSignal<String>,
    ws_connection: Arc<AsyncRwLock<Option<web_sys::WebSocket>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            pdsg_snapshot: RwSignal::new(None),
            alcoa_report: RwSignal::new(None),
            sensor_readings: RwSignal::new(Vec::new()),
            selected_node: RwSignal::new(None),
            loading: RwSignal::new(false),
            error: RwSignal::new(None),
            current_actor: RwSignal::new("frontend-user".into()),
            ws_connection: Arc::new(AsyncRwLock::new(None)),
        }
    }

    pub fn pdsg_snapshot(&self) -> ReadSignal<Option<PdsgSnapshot>> {
        self.pdsg_snapshot.read_only()
    }

    pub fn set_pdsg_snapshot(&self, snapshot: PdsgSnapshot) {
        self.pdsg_snapshot.set(Some(snapshot));
    }

    pub fn alcoa_report(&self) -> ReadSignal<Option<AlcoaReport>> {
        self.alcoa_report.read_only()
    }

    pub fn set_alcoa_report(&self, report: AlcoaReport) {
        self.alcoa_report.set(Some(report));
    }

    pub fn sensor_readings(&self) -> ReadSignal<Vec<SensorReading>> {
        self.sensor_readings.read_only()
    }

    pub fn add_sensor_reading(&self, reading: SensorReading) {
        self.sensor_readings.update(|readings| {
            readings.push(reading);
            if readings.len() > 1000 {
                readings.drain(0..readings.len() - 1000);
            }
        });
    }

    pub fn selected_node(&self) -> ReadSignal<Option<String>> {
        self.selected_node.read_only()
    }

    pub fn set_selected_node(&self, node_id: Option<String>) {
        self.selected_node.set(node_id);
    }

    pub fn loading(&self) -> ReadSignal<bool> {
        self.loading.read_only()
    }

    pub fn set_loading(&self, loading: bool) {
        self.loading.set(loading);
    }

    pub fn error(&self) -> ReadSignal<Option<String>> {
        self.error.read_only()
    }

    pub fn set_error(&self, error: Option<String>) {
        self.error.set(error);
    }

    pub fn current_actor(&self) -> ReadSignal<String> {
        self.current_actor.read_only()
    }

    pub fn set_current_actor(&self, actor: String) {
        self.current_actor.set(actor);
    }

    pub async fn set_ws_connection(&self, ws: Option<web_sys::WebSocket>) {
        let mut conn = self.ws_connection.write().await;
        *conn = ws;
    }

    pub async fn close_ws_connection(&self) {
        let mut conn = self.ws_connection.write().await;
        if let Some(ws) = conn.take() {
            let _ = ws.close();
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
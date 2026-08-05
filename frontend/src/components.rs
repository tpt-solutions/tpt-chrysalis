//! Reusable UI components

use crate::types::*;
use leptos::prelude::*;

#[component]
pub fn NodeCard(node: PdsgNode, on_click: Callback<String>) -> impl IntoView {
    let (node_id, label, status, color) = match &node {
        PdsgNode::Intent(n) => (
            n.node_id.clone(),
            format!("{:?}", n.intent_type),
            format!("{:?}", n.status),
            "#3b82f6", // blue
        ),
        PdsgNode::Actuator(n) => (
            n.node_id.clone(),
            format!("{:?} ({})", n.actuator_type, n.resource_id),
            format!("{:?}", n.status),
            "#10b981", // green
        ),
        PdsgNode::Sensor(n) => (
            n.node_id.clone(),
            format!("{:?} ({})", n.sensor_type, n.channel),
            format!("{:?}", n.status),
            "#f59e0b", // amber
        ),
    };

    let node_id_for_click = node_id.clone();
    view! {
        <div
            class="node-card"
            style=format!("border-left: 4px solid {};", color)
            on:click=move |_| on_click.run(node_id_for_click.clone())
        >
            <div class="node-label">{label}</div>
            <div class="node-status">{status}</div>
            <div class="node-id">{node_id}</div>
        </div>
    }
}

#[component]
pub fn SensorStream(sensor_id: String, readings: Vec<SensorReading>) -> impl IntoView {
    let latest = readings.last().cloned();

    view! {
        <div class="sensor-stream">
            <div class="sensor-header">
                <span class="sensor-id">{sensor_id}</span>
                <span class="sensor-live">"LIVE"</span>
            </div>
            <div class="sensor-value">
                {move || latest.as_ref().map(|r| format!("{:.2} {}", r.value, r.units)).unwrap_or_else(|| "—".into())}
            </div>
            <div class="sensor-history">
                {readings.iter().rev().take(20).map(|r| {
                    let reading = r.clone();
                    view! {
                        <div class="history-point" title=format!("{}ms: {:.2}", reading.timestamp_ms, reading.value)>
                            <div style=format!("height: {}px;", (reading.value * 10.0).clamp(1.0, 100.0))></div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
pub fn AlcoaCriterionRow(criterion: AlcoaCriterion) -> impl IntoView {
    let status_class = if criterion.satisfied { "pass" } else { "fail" };
    let status_text = if criterion.satisfied { "PASS" } else { "FAIL" };

    view! {
        <tr class=status_class>
            <td><code>{criterion.code}</code></td>
            <td>{criterion.name}</td>
            <td>{criterion.description}</td>
            <td class="status"><span class=status_class>{status_text}</span></td>
            <td class="evidence">{criterion.evidence}</td>
        </tr>
    }
}

#[component]
pub fn LedgerEntryRow(entry: ReportedEntry) -> impl IntoView {
    let outcome_class = if entry.outcome.starts_with("accepted") { "accepted" } else { "rejected" };
    let hash_short = format!("{}...", &entry.hash_hex[..16.min(entry.hash_hex.len())]);

    view! {
        <tr>
            <td>{entry.seq}</td>
            <td>{entry.timestamp_ms}</td>
            <td>{entry.actor}</td>
            <td>{entry.action}</td>
            <td class=outcome_class>{entry.outcome}</td>
            <td>{entry.scope}</td>
            <td><code>{hash_short}</code></td>
        </tr>
    }
}

#[component]
pub fn LoadingSpinner() -> impl IntoView {
    view! {
        <div class="loading-spinner">
            <div class="spinner"></div>
            <span>"Loading…"</span>
        </div>
    }
}

#[component]
pub fn ErrorDisplay(error: ReadSignal<Option<String>>) -> impl IntoView {
    view! {
        <Show when=move || error.get().is_some() fallback=|| ()>
            <div class="error-banner">
                <span>{move || error.get().unwrap_or_default()}</span>
                <button on:click=move |_| {
                    // We can't set on a ReadSignal, so we need a different approach
                    // For now, just log - in a real app you'd pass a callback
                    web_sys::console::log_1(&"Error dismissed".into());
                }>"Dismiss"</button>
            </div>
        </Show>
    }
}

#[component]
pub fn NavLink(href: &'static str, label: &'static str, active: ReadSignal<bool>) -> impl IntoView {
    view! {
        <a href=href class=move || if active.get() { "nav-link active" } else { "nav-link" }>
            {label}
        </a>
    }
}
//! Page components for each route

use crate::{
    api, organoid_viz,
    components::{AlcoaCriterionRow, ErrorDisplay, LedgerEntryRow, LoadingSpinner, NodeCard, SensorStream},
    state::AppState,
    types::*,
};
use leptos::html::Canvas;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::spawn_local;
use web_sys::window;

#[component]
pub fn Dashboard() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState not provided");
    let location = use_location();

    // Fetch PDSG snapshot on mount
    let fetch_snapshot = {
        let state = state.clone();
        move || {
            let state = state.clone();
            spawn_local(async move {
                state.set_loading(true);
                state.set_error(None);
                match api::fetch_pdsg_snapshot().await {
                    Ok(snapshot) => state.set_pdsg_snapshot(snapshot),
                    Err(e) => state.set_error(Some(e.message)),
                }
                state.set_loading(false);
            });
        }
    };

    // Clone for Effect
    let fetch_snapshot_for_effect = fetch_snapshot.clone();
    Effect::new(move |_| {
        location.pathname.get();
        fetch_snapshot_for_effect();
    });

    // Start WebSocket for sensor streams
    let state_for_ws = state.clone();
    Effect::new(move |_| {
        let state = state_for_ws.clone();
        spawn_local(async move {
            let state_for_handler = state.clone();
            match api::connect_sensor_websocket(move |msg| {
                let state = state_for_handler.clone();
                if let Ok(reading) = serde_json::from_value::<SensorReading>(msg) {
                    state.add_sensor_reading(reading);
                }
            }) {
                Ok(ws) => {
                    state.set_ws_connection(Some(ws)).await;
                }
                Err(e) => {
                    state.set_error(Some(format!("WebSocket error: {}", e.message)));
                }
            }
        });
    });

    // Create derived signals to avoid moving state in closures
    let pdsg_snapshot = state.pdsg_snapshot();
    let sensor_readings = state.sensor_readings();
    let alcoa_report = state.alcoa_report();
    let loading = state.loading();
    let error = state.error();

    // Create fetch_report as a function that doesn't capture state in a way that causes FnOnce
    let fetch_report = {
        let state = state.clone();
        move || {
            let state = state.clone();
            spawn_local(async move {
                state.set_loading(true);
                match api::fetch_alcoa_report(None).await {
                    Ok(report) => state.set_alcoa_report(report),
                    Err(e) => state.set_error(Some(e.message)),
                }
                state.set_loading(false);
            });
        }
    };

    // Clone fetch_report for use in button
    let fetch_report_for_button = fetch_report.clone();

    view! {
        <ErrorDisplay error=error />
        <div class="dashboard">
            <header class="dashboard-header">
                <h1>"System Dashboard"</h1>
                <div class="header-actions">
                    <Show when=move || loading.get() fallback=|| ()>
                        <LoadingSpinner />
                    </Show>
                </div>
            </header>

            <section class="dashboard-grid">
                <div class="card pdsg-summary">
                    <h2>"PDSG Summary"</h2>
                    <Show when=move || pdsg_snapshot.get().is_some() fallback=|| view! { <LoadingSpinner /> }>
                        {move || {
                            let snapshot = pdsg_snapshot.get().unwrap();
                            let intent_count = snapshot.nodes.iter().filter(|n| matches!(n, PdsgNode::Intent(_))).count();
                            let actuator_count = snapshot.nodes.iter().filter(|n| matches!(n, PdsgNode::Actuator(_))).count();
                            let sensor_count = snapshot.nodes.iter().filter(|n| matches!(n, PdsgNode::Sensor(_))).count();
                            view! {
                                <dl>
                                    <dt>"Total Nodes"</dt><dd>{snapshot.nodes.len()}</dd>
                                    <dt>"Intents"</dt><dd>{intent_count}</dd>
                                    <dt>"Actuators"</dt><dd>{actuator_count}</dd>
                                    <dt>"Sensors"</dt><dd>{sensor_count}</dd>
                                    <dt>"Edges"</dt><dd>{snapshot.edges.len()}</dd>
                                    <dt>"Last Updated"</dt><dd>{snapshot.timestamp_ms} ms</dd>
                                </dl>
                            }
                        }}
                    </Show>
                </div>

                <div class="card sensor-summary">
                    <h2>"Live Sensors"</h2>
                    <Show when=move || !sensor_readings.get().is_empty() fallback=|| view! { <p>"No sensor data yet"</p> }>
                        {move || {
                            let readings = sensor_readings.get();
                            let mut by_sensor: std::collections::HashMap<String, Vec<SensorReading>> = std::collections::HashMap::new();
                            for r in readings {
                                by_sensor.entry(r.sensor_id.clone()).or_default().push(r);
                            }
                            by_sensor.into_iter().take(4).map(|(sensor_id, readings)| {
                                view! { <SensorStream sensor_id=sensor_id readings=readings /> }
                            }).collect_view()
                        }}
                    </Show>
                </div>

                <div class="card compliance-summary">
                    <h2>"Compliance Status"</h2>
                    {move || {
                        if alcoa_report.get().is_some() {
                            let report = alcoa_report.get().unwrap();
                            view! {
                                <dl>
                                    <dt>"Core Compliant"</dt><dd class=if report.core_compliant { "pass" } else { "fail" }>{if report.core_compliant { "YES" } else { "NO" }}</dd>
                                    <dt>"Ledger Entries"</dt><dd>{report.ledger_entry_count}</dd>
                                    <dt>"Reported Entries"</dt><dd>{report.reported_entry_count}</dd>
                                    <dt>"Chain Integrity"</dt><dd class=if report.integrity.is_intact { "pass" } else { "fail" }>{if report.integrity.is_intact { "INTACT" } else { "BROKEN" }}</dd>
                                </dl>
                            }.into_any()
                        } else {
                            let fetch_report = fetch_report_for_button.clone();
                            view! { <button on:click=move |_| fetch_report()>"Load Report"</button> }.into_any()
                        }
                    }}
                </div>

                <div class="card quick-actions">
                    <h2>"Quick Actions"</h2>
                    <div class="action-buttons">
                        <button on:click=move |_| fetch_snapshot()>"Refresh PDSG"</button>
                        <button on:click=move |_| fetch_report()>"Refresh Compliance"</button>
                        <a href="/pdsg" class="button-link">"Explore PDSG"</a>
                        <a href="/compliance" class="button-link">"Full Report"</a>
                    </div>
                </div>
            </section>
        </div>
    }
}

#[component]
pub fn PdsgExplorer() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState not provided");
    let location = use_location();

    let fetch_snapshot = {
        let state = state.clone();
        move || {
            let state = state.clone();
            spawn_local(async move {
                state.set_loading(true);
                state.set_error(None);
                match api::fetch_pdsg_snapshot().await {
                    Ok(snapshot) => state.set_pdsg_snapshot(snapshot),
                    Err(e) => state.set_error(Some(e.message)),
                }
                state.set_loading(false);
            });
        }
    };

    // Clone for Effect
    let fetch_snapshot_for_effect = fetch_snapshot.clone();
    Effect::new(move |_| {
        location.pathname.get();
        fetch_snapshot_for_effect();
    });

    let pdsg_snapshot = state.pdsg_snapshot();
    let selected_node = state.selected_node();
    let loading = state.loading();
    let error = state.error();

    let on_select = {
        let s = state.clone();
        Callback::new(move |id: String| s.set_selected_node(Some(id)))
    };

    view! {
        <ErrorDisplay error=error />
        <div class="pdsg-explorer">
            <header class="page-header">
                <h1>"PDSG Explorer"</h1>
                <div class="header-actions">
                    <Show when=move || loading.get() fallback=|| ()>
                        <LoadingSpinner />
                    </Show>
                    <button on:click=move |_| fetch_snapshot()>"Refresh"</button>
                </div>
            </header>

            <div class="explorer-layout">
                <aside class="node-list">
                    <h3>"Nodes"</h3>
                    <Show when=move || pdsg_snapshot.get().is_some() fallback=|| view! { <LoadingSpinner /> }>
                        {move || {
                            let snapshot = pdsg_snapshot.get().unwrap();
                            snapshot.nodes.into_iter().map(|node| {
                                view! {
                                    <NodeCard
                                        node=node
                                        on_click=on_select.clone()
                                    />
                                }
                            }).collect_view()
                        }}
                    </Show>
                </aside>

                <main class="node-detail">
                    <Show when=move || selected_node.get().is_some() fallback=|| view! {
                        <div class="detail-placeholder">
                            <p>"Select a node from the list to view details"</p>
                        </div>
                    }>
                        {move || {
                            let snapshot = pdsg_snapshot.get().unwrap();
                            let node_id = selected_node.get().unwrap();
                            snapshot.nodes.into_iter().find(|n| {
                                let id = match n {
                                    PdsgNode::Intent(n) => &n.node_id,
                                    PdsgNode::Actuator(n) => &n.node_id,
                                    PdsgNode::Sensor(n) => &n.node_id,
                                };
                                *id == node_id
                            }).map(|node| {
                                view! { <NodeDetail node=node edges=snapshot.edges.clone() /> }
                            })
                        }}
                    </Show>
                </main>
            </div>
        </div>
    }
}

#[component]
fn NodeDetail(node: PdsgNode, edges: Vec<Edge>) -> impl IntoView {
    let incoming: Vec<_> = edges.iter().filter(|e| e.target_node_id == get_node_id(&node)).collect();
    let outgoing: Vec<_> = edges.iter().filter(|e| e.source_node_id == get_node_id(&node)).collect();

    view! {
        <div class="node-detail-card">
            <h2>"Node Details"</h2>
            <div class="detail-fields">
                {match &node {
                    PdsgNode::Intent(n) => view! {
                        <dl>
                            <dt>"Type"</dt><dd>{format!("{:?}", n.intent_type)}</dd>
                            <dt>"Status"</dt><dd>{format!("{:?}", n.status)}</dd>
                            <dt>"Created By"</dt><dd>{n.created_by.clone()}</dd>
                            <dt>"Created At"</dt><dd>{n.created_at_ms} ms</dd>
                            <dt>"Parameters"</dt><dd><pre>{serde_json::to_string_pretty(&n.parameters).unwrap_or_default()}</pre></dd>
                        </dl>
                    }.into_any(),
                    PdsgNode::Actuator(n) => view! {
                        <dl>
                            <dt>"Actuator Type"</dt><dd>{format!("{:?}", n.actuator_type)}</dd>
                            <dt>"Resource ID"</dt><dd>{n.resource_id.clone()}</dd>
                            <dt>"Status"</dt><dd>{format!("{:?}", n.status)}</dd>
                            <dt>"Capabilities"</dt><dd>{n.capabilities.iter().map(|c| format!("{:?}", c)).collect::<Vec<_>>().join(", ")}</dd>
                            <dt>"Location"</dt><dd>{format!("({}, {}, {})", n.location.x, n.location.y, n.location.z)}</dd>
                        </dl>
                    }.into_any(),
                    PdsgNode::Sensor(n) => view! {
                        <dl>
                            <dt>"Sensor Type"</dt><dd>{format!("{:?}", n.sensor_type)}</dd>
                            <dt>"Channel"</dt><dd>{n.channel.clone()}</dd>
                            <dt>"Units"</dt><dd>{n.units.clone()}</dd>
                            <dt>"Sample Rate"</dt><dd>{format!("{} Hz", n.sample_rate_hz)}</dd>
                            <dt>"Status"</dt><dd>{format!("{:?}", n.status)}</dd>
                            <dt>"Location"</dt><dd>{format!("({}, {}, {})", n.location.x, n.location.y, n.location.z)}</dd>
                        </dl>
                    }.into_any(),
                }}
            </div>

            <div class="edge-section">
                <h3>"Incoming Edges"</h3>
                <ul>{incoming.iter().map(|e| view! { <li>{format!("{} → {} ({:?})", e.source_node_id, e.target_node_id, e.edge_kind)}</li> }).collect_view() }</ul>
            </div>

            <div class="edge-section">
                <h3>"Outgoing Edges"</h3>
                <ul>{outgoing.iter().map(|e| view! { <li>{format!("{} → {} ({:?})", e.source_node_id, e.target_node_id, e.edge_kind)}</li> }).collect_view() }</ul>
            </div>
        </div>
    }
}

fn get_node_id(node: &PdsgNode) -> &str {
    match node {
        PdsgNode::Intent(n) => &n.node_id,
        PdsgNode::Actuator(n) => &n.node_id,
        PdsgNode::Sensor(n) => &n.node_id,
    }
}

#[component]
fn ComplianceReportContent(report: AlcoaReport) -> impl IntoView {
    view! {
        <div class="report-header">
            <dl>
                <dt>"Report ID"</dt><dd>{report.report_id}</dd>
                <dt>"Generated"</dt><dd>{report.generated_at_ms} ms</dd>
                <dt>"Actor Filter"</dt><dd>{report.actor_filter.unwrap_or_else(|| "none".into())}</dd>
                <dt>"Ledger Entries"</dt><dd>{report.ledger_entry_count}</dd>
                <dt>"Reported Entries"</dt><dd>{report.reported_entry_count}</dd>
                <dt>"Core Compliant"</dt><dd class=if report.core_compliant { "pass" } else { "fail" }>{if report.core_compliant { "YES" } else { "NO" }}</dd>
                <dt>"Chain Integrity"</dt><dd class=if report.integrity.is_intact { "pass" } else { "fail" }>{if report.integrity.is_intact { "INTACT" } else { "BROKEN" }}</dd>
                <dt>"Last Hash"</dt><dd><code>{report.last_hash_hex}</code></dd>
            </dl>
        </div>

        <section class="criteria-section">
            <h2>"ALCOA+ Criteria"</h2>
            <table>
                <thead>
                    <tr>
                        <th>"Code"</th>
                        <th>"Name"</th>
                        <th>"Description"</th>
                        <th>"Status"</th>
                        <th>"Evidence"</th>
                    </tr>
                </thead>
                <tbody>
                    {report.criteria.iter().map(|c| view! { <AlcoaCriterionRow criterion=c.clone() /> }).collect_view() }
                </tbody>
            </table>
        </section>

        <section class="history-section">
            <h2>"Attributable History"</h2>
            <table>
                <thead>
                    <tr>
                        <th>"Seq"</th>
                        <th>"Timestamp"</th>
                        <th>"Actor"</th>
                        <th>"Action"</th>
                        <th>"Outcome"</th>
                        <th>"Scope"</th>
                        <th>"Hash"</th>
                    </tr>
                </thead>
                <tbody>
                    {report.history.iter().map(|e| view! { <LedgerEntryRow entry=e.clone() /> }).collect_view() }
                </tbody>
            </table>
        </section>
    }
}

#[component]
pub fn ComplianceReport() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState not provided");
    let location = use_location();

    // Create fetch_report as a function that doesn't capture state in a way that causes FnOnce
    let fetch_report = {
        let state = state.clone();
        move |actor: Option<String>| {
            let state = state.clone();
            spawn_local(async move {
                state.set_loading(true);
                state.set_error(None);
                match api::fetch_alcoa_report(actor.as_deref()).await {
                    Ok(report) => state.set_alcoa_report(report),
                    Err(e) => state.set_error(Some(e.message)),
                }
                state.set_loading(false);
            });
        }
    };

    // Create a separate closure for the Effect to avoid move issues
    let fetch_report_for_effect = {
        let state = state.clone();
        move |actor: Option<String>| {
            let state = state.clone();
            spawn_local(async move {
                state.set_loading(true);
                state.set_error(None);
                match api::fetch_alcoa_report(actor.as_deref()).await {
                    Ok(report) => state.set_alcoa_report(report),
                    Err(e) => state.set_error(Some(e.message)),
                }
                state.set_loading(false);
            });
        }
    };
    Effect::new(move |_| {
        location.pathname.get();
        fetch_report_for_effect(None);
    });

    let alcoa_report = state.alcoa_report();
    let loading = state.loading();
    let error = state.error();

    view! {
        <ErrorDisplay error=error />
        <div class="compliance-report">
            <header class="page-header">
                <h1>"ALCOA+ Compliance Report"</h1>
                <div class="header-actions">
                    <Show when=move || loading.get() fallback=|| ()>
                        <LoadingSpinner />
                    </Show>
                    <button on:click=move |_| fetch_report(None)>"Refresh"</button>
                </div>
            </header>

            <Show when=move || alcoa_report.get().is_some() fallback=|| view! { <LoadingSpinner /> }>
                {move || {
                    let report = alcoa_report.get().unwrap();
                    view! { <ComplianceReportContent report=report /> }
                }}
            </Show>
        </div>
    }
}

#[component]
pub fn Settings() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState not provided");

    view! {
        <div class="settings-page">
            <header class="page-header">
                <h1>"Settings"</h1>
            </header>

            <section class="settings-section">
                <h2>"Actor Identity"</h2>
                <div class="setting-row">
                    <label>"Current Actor: "</label>
                    <input
                        type="text"
                        value=state.current_actor()
                        on:input=move |ev| state.set_current_actor(event_target_value(&ev))
                    />
                </div>
            </section>

            <section class="settings-section">
                <h2>"API Configuration"</h2>
                <p>"API Base: /api/v1 (configured at build time)"</p>
            </section>

            <section class="settings-section">
                <h2>"WebSocket"</h2>
                <p>"Sensor stream: /api/v1/ws/sensors (auto-connects)"</p>
            </section>
        </div>
    }
}

#[component]
pub fn OrganoidArray() -> impl IntoView {
    let state = use_context::<AppState>().expect("AppState not provided");
    let location = use_location();

    let canvas_ref = NodeRef::<Canvas>::new();
    let sensor_positions = RwSignal::new(Vec::<(String, Location3D)>::new());
    let sensor_values = RwSignal::new(HashMap::<String, f64>::new());
    let angle = RwSignal::new(0.0f64);
    let stop = RwSignal::new(false);

    // Fetch the PDSG snapshot to learn the (simulated) chip/sensor world positions.
    let fetch_snapshot = {
        let sensor_positions = sensor_positions.clone();
        move || {
            let sensor_positions = sensor_positions.clone();
            spawn_local(async move {
                if let Ok(snapshot) = api::fetch_pdsg_snapshot().await {
                    let sensors: Vec<(String, Location3D)> = snapshot
                        .nodes
                        .iter()
                        .filter_map(|n| match n {
                            PdsgNode::Sensor(s) => Some((s.node_id.clone(), s.location.clone())),
                            _ => None,
                        })
                        .collect();
                    sensor_positions.set(sensors);
                }
            });
        }
    };
    let fetch_snapshot_for_effect = fetch_snapshot.clone();
    Effect::new(move |_| {
        location.pathname.get();
        fetch_snapshot_for_effect();
    });

    // Connect to the live sensor WebSocket; feed both the global state and the
    // local per-sensor value map that drives the 3D colouring.
    let state_ws = state.clone();
    let sensor_values_ws = sensor_values.clone();
    Effect::new(move |_| {
        let state = state_ws.clone();
        let sensor_values = sensor_values_ws.clone();
        spawn_local(async move {
            let state_for_ws = state.clone();
            let sensor_values_for_ws = sensor_values.clone();
            match api::connect_sensor_websocket(move |msg| {
                if let Ok(reading) = serde_json::from_value::<SensorReading>(msg) {
                    state_for_ws.add_sensor_reading(reading.clone());
                    sensor_values_for_ws.update(|m| {
                        m.insert(reading.sensor_id.clone(), reading.value);
                    });
                }
            }) {
                Ok(ws) => state.set_ws_connection(Some(ws)).await,
                Err(e) => state.set_error(Some(format!("WebSocket error: {}", e.message))),
            }
        });
    });

    // Drive a requestAnimationFrame loop that re-projects and redraws the array,
    // synced to the live sensor value map. Set up inline (not in an Effect) so the
    // self-rescheduling closure can hold a non-`Send` `Rc` handle safely on wasm.
    let canvas_loop = canvas_ref.clone();
    let sensor_positions_loop = sensor_positions.clone();
    let sensor_values_loop = sensor_values.clone();
    let angle_loop = angle.clone();
    let stop_loop = stop.clone();
    let frame: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = frame.clone();
    *frame.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        if stop_loop.get() {
            return;
        }
        if let Some(canvas) = canvas_loop.get() {
            if let Some(ctx) = organoid_viz::canvas_2d(&canvas) {
                let positions = sensor_positions_loop.get();
                let values = sensor_values_loop.get();
                let nodes = organoid_viz::build_nodes(&positions, &values);
                let next = angle_loop.get() + 0.008;
                angle_loop.set(next);
                let (lo, hi) = value_range(&values);
                organoid_viz::render_scene(&ctx, &canvas, &nodes, next, lo, hi);
            }
        }
        if let Some(win) = window() {
            let _ = win.request_animation_frame(
                g.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
            );
        }
    }) as Box<dyn FnMut()>));
    if let Some(win) = window() {
        let _ = win.request_animation_frame(
            frame.borrow().as_ref().unwrap().as_ref().unchecked_ref(),
        );
    }
    on_cleanup(move || stop.set(true));

    let sensor_values_view = sensor_values.clone();
    let legend_values = move || {
        let values = sensor_values_view.get();
        let mut out: Vec<(String, f64)> = values.into_iter().collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    };

    view! {
        <ErrorDisplay error=state.error() />
        <div class="organoid-view">
            <header class="page-header">
                <h1>"Organ-on-a-Chip Array"</h1>
                <div class="header-actions">
                    <button on:click=move |_| fetch_snapshot()>"Refresh Layout"</button>
                </div>
            </header>

            <p class="organoid-hint">
                "3D array of simulated OoC chips, rotated continuously and coloured by live sensor value (blue = low, red = high). Synced to the real-time sensor WebSocket."
            </p>

            <div class="organoid-layout">
                <canvas
                    node_ref=canvas_ref
                    class="organoid-canvas"
                    width="880"
                    height="520"
                ></canvas>

                <aside class="organoid-legend">
                    <h3>"Live Sensor Values"</h3>
                    <Show when=move || !legend_values().is_empty() fallback=|| view! { <p>"Awaiting sensor data…"</p> }>
                        <ul>
                            {move || {
                                legend_values()
                                    .into_iter()
                                    .map(|(id, v)| {
                                        view! { <li><code>{id}</code> ": " {format!("{:.3}", v)}</li> }
                                    })
                                    .collect_view()
                            }}
                        </ul>
                    </Show>
                </aside>
            </div>
        </div>
    }
}

fn value_range(values: &HashMap<String, f64>) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    let mut iter = values.values();
    let first = *iter.next().unwrap();
    let (mut lo, mut hi) = (first, first);
    for v in iter {
        if *v < lo {
            lo = *v;
        }
        if *v > hi {
            hi = *v;
        }
    }
    if (hi - lo).abs() < 1e-9 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    }
}
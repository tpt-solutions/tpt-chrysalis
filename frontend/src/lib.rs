//! `tpt-chrysalis-frontend`: Leptos-based Rust/Wasm frontend for real-time
//! microfluidic-state visualization, PDSG exploration, and ALCOA+ compliance reporting.

pub mod api;
pub mod components;
pub mod pages;
pub mod state;
pub mod types;

use leptos::prelude::*;
use leptos_router::components::*;

#[component]
fn NotFound() -> impl IntoView {
    view! { <div class="not-found">"Page not found"</div> }
}

#[component]
pub fn App() -> impl IntoView {
    provide_context(state::AppState::new());

    view! {
        <Router>
            <nav class="navbar">
                <div class="nav-brand">
                    <a href="/">"tpt-chrysalis"</a>
                </div>
                <div class="nav-links">
                    <A href="/">"Dashboard"</A>
                    <A href="/pdsg">"PDSG Explorer"</A>
                    <A href="/compliance">"Compliance"</A>
                    <A href="/settings">"Settings"</A>
                </div>
            </nav>
            <main class="main-content">
                <Routes fallback=NotFound>
                    <Route path=() view=pages::Dashboard />
                    <Route path=("pdsg",) view=pages::PdsgExplorer />
                    <Route path=("compliance",) view=pages::ComplianceReport />
                    <Route path=("settings",) view=pages::Settings />
                </Routes>
            </main>
        </Router>
    }
}
use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Shared state between the control loop and the UI
#[derive(Clone)]
pub struct DashboardState {
    pub power: f64,
    pub voltage: f64,
    pub current: f64,
    pub duty: f64,
    pub forecast: f64,
    pub irradiance_wm2: f64,
    pub sim_hour: f64,
    pub power_history: VecDeque<(f64, f64)>, // (time, power)
    pub forecast_history: VecDeque<(f64, f64)>, // (time, forecast)
}

impl Default for DashboardState {
    fn default() -> Self {
        Self {
            power: 0.0,
            voltage: 0.0,
            current: 0.0,
            duty: 0.0,
            forecast: 1.0,
            irradiance_wm2: 0.0,
            sim_hour: 0.0,
            power_history: VecDeque::with_capacity(1000),
            forecast_history: VecDeque::with_capacity(1000),
        }
    }
}

pub struct HeliosDashboard {
    state: Arc<Mutex<DashboardState>>,
}

impl HeliosDashboard {
    pub fn new(cc: &eframe::CreationContext<'_>, state: Arc<Mutex<DashboardState>>) -> Self {
        // Optional: configure fonts, visuals here
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        Self { state }
    }
}

impl eframe::App for HeliosDashboard {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = {
            let guard = self.state.lock().unwrap();
            guard.clone()
        };

        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.heading("HELIOS-NODE — Predictive DC-Microgrid MPPT");
            ui.label("Real-time telemetry and AI forecast");
        });

        egui::SidePanel::left("left_panel").min_width(200.0).show(ctx, |ui| {
            ui.heading("Power Gauge");
            ui.separator();
            ui.label(format!("Time (Simulated): {:02.0}:00", state.sim_hour % 24.0));
            ui.add_space(10.0);
            
            ui.label(egui::RichText::new("Power").strong());
            ui.label(egui::RichText::new(format!("{:.1} W", state.power)).size(24.0).color(egui::Color32::from_rgb(0, 255, 0)));
            ui.add_space(5.0);

            ui.label(egui::RichText::new("Voltage").strong());
            ui.label(egui::RichText::new(format!("{:.1} V", state.voltage)).size(18.0).color(egui::Color32::LIGHT_BLUE));
            ui.add_space(5.0);

            ui.label(egui::RichText::new("Current").strong());
            ui.label(egui::RichText::new(format!("{:.1} A", state.current)).size(18.0).color(egui::Color32::LIGHT_YELLOW));
            ui.add_space(10.0);

            ui.separator();
            ui.heading("AI Status");
            ui.label(format!("Forecast: {:.2}", state.forecast));
            ui.label(format!("Irradiance: {:.0} W/m²", state.irradiance_wm2));
            ui.label(format!("Duty Cycle: {:.2}", state.duty));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Timeline");
            
            let power_points: PlotPoints = state.power_history.iter().copied().map(|(t, v)| [t, v]).collect();
            let forecast_points: PlotPoints = state.forecast_history.iter().copied().map(|(t, v)| [t, v * 380.0]).collect(); // Scale forecast to power range
            
            let power_line = Line::new(power_points).name("Power (W)").color(egui::Color32::from_rgb(0, 255, 0));
            let forecast_line = Line::new(forecast_points).name("AI Forecast (Scaled)").color(egui::Color32::from_rgb(255, 100, 0));
            
            Plot::new("telemetry_plot")
                .view_aspect(2.0)
                .legend(egui_plot::Legend::default())
                .show(ui, |plot_ui| {
                    plot_ui.line(power_line);
                    plot_ui.line(forecast_line);
                });
        });

        // Request a repaint to update the UI continuously
        ctx.request_repaint();
    }
}

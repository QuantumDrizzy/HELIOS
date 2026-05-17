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
    pub pmax: f64,
    pub location: String,
    pub currency: String,
    pub battery_type: String,
    pub soc: f64,
    pub inverter_v_ac: f64,
    pub inverter_eff: f64,
    pub protections_ok: bool,
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
            pmax: 380.0,
            location: "Unknown Station".to_string(),
            currency: "USD".to_string(),
            battery_type: "Unknown".to_string(),
            soc: 0.0,
            inverter_v_ac: 0.0,
            inverter_eff: 0.0,
            protections_ok: true,
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
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("HELIOS-NODE").strong().color(egui::Color32::from_rgb(255, 50, 50)));
                ui.label(egui::RichText::new("— Sovereign Energy OS").italics());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(format!("Station: {}", state.location)).small().color(egui::Color32::GRAY));
                });
            });
            ui.add_space(5.0);
        });

        egui::SidePanel::left("left_panel").min_width(220.0).show(ctx, |ui| {
            ui.add_space(10.0);
            ui.heading("Power Metrics");
            ui.separator();
            ui.add_space(5.0);

            ui.horizontal(|ui| {
                ui.label("Simulated Time:");
                ui.label(egui::RichText::new(format!("{:02.0}:00", state.sim_hour % 24.0)).strong().monospace());
            });
            ui.add_space(10.0);
            
            ui.group(|ui| {
                ui.label(egui::RichText::new("CURRENT POWER").small().strong());
                ui.label(egui::RichText::new(format!("{:.1} W", state.power)).size(32.0).color(egui::Color32::from_rgb(0, 255, 150)));
                
                let progress = (state.power / state.pmax) as f32;
                ui.add(egui::ProgressBar::new(progress).show_percentage().fill(egui::Color32::from_rgb(0, 150, 255)));
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Voltage").small());
                    ui.label(egui::RichText::new(format!("{:.1} V", state.voltage)).strong().color(egui::Color32::LIGHT_BLUE));
                });
                ui.add_space(20.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Current").small());
                    ui.label(egui::RichText::new(format!("{:.1} A", state.current)).strong().color(egui::Color32::LIGHT_YELLOW));
                });
            });

            ui.add_space(20.0);
            ui.heading("Energy Storage");
            ui.separator();
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Chemistry:");
                    ui.label(egui::RichText::new(&state.battery_type).strong().color(egui::Color32::from_rgb(200, 200, 255)));
                });
                ui.label(egui::RichText::new("State of Charge (SoC)").small());
                ui.add(egui::ProgressBar::new(state.soc as f32).show_percentage().fill(egui::Color32::from_rgb(50, 200, 50)));
            });

            ui.add_space(20.0);
            ui.heading("System Health");
            ui.separator();
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Protections:");
                    if state.protections_ok {
                        ui.label(egui::RichText::new("OK").strong().color(egui::Color32::GREEN));
                    } else {
                        ui.label(egui::RichText::new("FAULT").strong().color(egui::Color32::RED));
                    }
                });
                ui.label(format!("Inverter AC: {:.1} V", state.inverter_v_ac));
                ui.label(format!("Inverter Eff: {:.1}%", state.inverter_eff * 100.0));
            });

            ui.add_space(20.0);
            ui.heading("Economic Optimization");
            ui.separator();
            
            let hour = state.sim_hour % 24.0;
            let is_peak = hour >= 7.0 && hour < 20.0;
            // Note: In a real system, these would be fed from config or API
            // For now we use the dynamic currency from state
            let tariff_label = if is_peak { "PEAK" } else { "OFF-PEAK" };

            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label("Tariff Zone:");
                    if is_peak {
                        ui.label(egui::RichText::new(tariff_label).strong().color(egui::Color32::RED));
                    } else {
                        ui.label(egui::RichText::new(tariff_label).strong().color(egui::Color32::GREEN));
                    }
                });
                ui.label(format!("Price: Estimating... [{}]", state.currency));
            });

            ui.add_space(10.0);
            ui.label(egui::RichText::new("AI Prediction Status").strong());
            ui.label(format!("Irradiance: {:.0} W/m²", state.irradiance_wm2));
            ui.label(format!("Forecast Bias: {:.2}", state.forecast));
            ui.label(format!("Duty Cycle: {:.2}", state.duty));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Timeline");
            
            let power_points: PlotPoints = state.power_history.iter().copied().map(|(t, v)| [t, v]).collect();
            let forecast_points: PlotPoints = state.forecast_history.iter().copied().map(|(t, v)| [t, v * state.pmax]).collect(); // Scale forecast to pmax range
            
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

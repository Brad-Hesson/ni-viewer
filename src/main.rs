use std::{ops::RangeInclusive, time::Duration};

use eframe::egui::{
    self,
    plot::{Legend, Line, Plot, PlotBounds},
    Key, Ui,
};
use itertools::Itertools;
use ni_usb_6259::tasks::ContinuousAquisitionTask;

const PROBE_CHANNEL: &str = "Dev3/ai0";
const VOLTAGE_CHANNEL: &str = "Dev3/ai1";

fn main() {
    let size = egui::vec2(16., 9.) * 80.;
    let options = eframe::NativeOptions {
        initial_window_size: Some(size),
        ..Default::default()
    };
    eframe::run_native("Viewer", options, Box::new(|cc| Box::new(App::new(cc)))).unwrap();
}

struct App {
    running: bool,
    task: ni_usb_6259::tasks::ContinuousAquisitionTask<2>,
    readings: [Vec<f64>; 2],
    plot: ChannelPlot<2>,
}
impl App {
    fn new(_cc: &eframe::CreationContext) -> Self {
        let sample_rate = 50_000.;
        let task = ContinuousAquisitionTask::new(
            "",
            [PROBE_CHANNEL, VOLTAGE_CHANNEL],
            sample_rate,
            Duration::from_secs(1),
        )
        .unwrap();
        let channel_one = Channel::new("Displacement", 1.);
        let channel_two = Channel::new("Voltage", 1.);
        Self {
            task,
            readings: [vec![], vec![]],
            running: false,
            plot: ChannelPlot::new([channel_one, channel_two], sample_rate),
        }
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.running {
            let all_readings = self.task.read_samples().unwrap();
            for (vec, readings) in self.readings.iter_mut().zip(all_readings) {
                vec.extend(readings);
            }
            ctx.request_repaint();
        }
        egui::panel::SidePanel::left("left_panel")
            .resizable(true)
            .show(ctx, |ui| {
                let button_text = if self.running { "Stop" } else { "Start" };
                if ui.button(button_text).clicked() {
                    if self.running {
                        self.running = false;
                        self.task.stop().unwrap();
                    } else {
                        self.running = true;
                        self.task.start().unwrap();
                    }
                }
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            if ui.input(|i| i.key_pressed(Key::Num0)) {
                self.plot.active = 0;
            }
            if ui.input(|i| i.key_pressed(Key::Num1)) {
                self.plot.active = 1;
            }
            let data = self.readings.iter().map(|v| v.as_slice()).collect_vec();
            self.plot.show(data.try_into().unwrap(), ui)
        });
    }
}

struct ChannelPlot<const N: usize> {
    channels: [Channel; N],
    active: usize,
    plot_time: f64,
    sample_rate: f64,
    points_per_channel: usize,
}
impl<const N: usize> ChannelPlot<N> {
    fn new(channels: [Channel; N], sample_rate: f64) -> Self {
        Self {
            channels,
            active: 0,
            plot_time: 10.,
            sample_rate,
            points_per_channel: 1000,
        }
    }
    fn show(&mut self, data: [&[f64]; N], ui: &mut Ui) {
        let legend = Legend::default()
            .position(egui::plot::Corner::RightTop)
            .text_style(egui::TextStyle::Heading);
        Plot::new("plot")
            .legend(legend)
            .allow_drag(false)
            .allow_boxed_zoom(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .x_axis_formatter(time_formatter)
            .y_axis_formatter(metric_formatter)
            .show(ui, |plot_ui| {
                let active_channel_mut = &mut self.channels[self.active];
                if plot_ui.plot_hovered() {
                    let scroll = plot_ui.ctx().input(|i| i.scroll_delta[1]);
                    let space = plot_ui.ctx().input(|i| i.key_down(Key::Space));
                    if space {
                        self.plot_time /= 1.005f64.powf(scroll as _);
                    } else {
                        active_channel_mut.zoom /= 1.005f64.powf(scroll as _);
                    }
                }
                active_channel_mut.pos -= plot_ui.pointer_coordinate_drag_delta()[1] as f64;
                let x_min = -self.plot_time;
                let y_min = -1. * active_channel_mut.zoom + active_channel_mut.pos;
                let x_max = self.plot_time * 0.1;
                let y_max = 1. * active_channel_mut.zoom + active_channel_mut.pos;
                let bounds = PlotBounds::from_min_max([x_min, y_min], [x_max, y_max]);
                plot_ui.set_plot_bounds(bounds);
                let active_channel = &self.channels[self.active];
                let values_per_window = (self.plot_time * self.sample_rate) as usize;
                let group_size = (values_per_window / self.points_per_channel).max(1);
                let first_index =
                    data[0].len().saturating_sub(values_per_window) / group_size * group_size;
                for (i, channel) in self.channels.iter().enumerate() {
                    let values = &data[i][first_index..];
                    let mut points = Vec::with_capacity(self.points_per_channel);
                    for i in 0..(values.len() / group_size) {
                        let t = ((i * group_size) as f64 - values.len() as f64) / self.sample_rate;
                        let y = values[(i * group_size)..((i + 1) * group_size)]
                            .iter()
                            .sum::<f64>()
                            / group_size as f64;
                        points.push([
                            t,
                            (y - channel.pos) / channel.zoom * active_channel.zoom
                                + active_channel.pos,
                        ])
                    }
                    let mut line = Line::new(points).name(&channel.name);
                    if self.active == i {
                        line = line.highlight(true);
                    }
                    plot_ui.line(line);
                }
            });
    }
}

fn metric_formatter(v: f64, range: &RangeInclusive<f64>) -> String {
    let len = range.end() - range.start();
    if len <= 1e-6 {
        format!("{:.1} n", v / 1e-9)
    } else if len <= 1e-3 {
        format!("{:.1} u", v / 1e-6)
    } else if len <= 1. {
        format!("{:.1} m", v / 1e-3)
    } else {
        format!("{} hrs", (v / 60. / 60.) as isize)
    }
}

fn time_formatter(v: f64, range: &RangeInclusive<f64>) -> String {
    let len = range.end() - range.start();
    if len <= 60. * 2. {
        format!("{:.1} secs", v)
    } else if len <= 60. * 60. * 2. {
        format!("{} mins", (v / 60.) as isize)
    } else {
        format!("{} hrs", (v / 60. / 60.) as isize)
    }
}
struct Channel {
    name: String,
    zoom: f64,
    pos: f64,
}
impl Channel {
    fn new(name: &str, initial_zoom: f64) -> Self {
        Self {
            name: name.to_string(),
            zoom: initial_zoom,
            pos: 0.,
        }
    }
}

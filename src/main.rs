use eframe::egui;
use egui_plot::{Line, Plot, PlotPoints};
use sysinfo::{Pid, Process, ProcessesToUpdate, System};

struct ProcessExplorerApp {
    system: System,
    selected_pid: Option<Pid>,
    filter_text: String,
    sort_column: SortColumn,
    sort_ascending: bool,
    refresh_interval: std::time::Duration,
    last_refresh: std::time::Instant,
    visible_columns: VisibleColumns,
    // Historical data for mini graphs
    cpu_history: Vec<f64>,
    memory_history: Vec<f64>,
    max_history_points: usize,
    // Resource monitoring window
    show_resource_window: bool,
    resource_window: ResourceWindow,
}

#[derive(Default)]
struct ResourceWindow {
    selected_tab: ResourceTab,
    show_one_graph_per_cpu: bool,
    cpu_core_histories: Vec<Vec<f64>>,
    memory_history: Vec<f64>,
    io_read_history: Vec<f64>,
    io_write_history: Vec<f64>,
}

#[derive(Default, Clone, Copy, PartialEq)]
enum ResourceTab {
    #[default]
    Summary,
    Cpu,
    Memory,
    Io,
}

#[derive(Clone)]
struct VisibleColumns {
    virtual_memory: bool,
    parent_pid: bool,
    start_time: bool,
    executable_path: bool,
    working_directory: bool,
}

impl Default for VisibleColumns {
    fn default() -> Self {
        Self {
            virtual_memory: false,
            parent_pid: false,
            start_time: false,
            executable_path: false,
            working_directory: false,
        }
    }
}

#[derive(Default, Clone, Copy, PartialEq)]
enum SortColumn {
    #[default]
    Name,
    Pid,
    Cpu,
    Memory,
    Status,
    VirtualMemory,
    ParentPid,
    StartTime,
    ExecutablePath,
    WorkingDirectory,
}

impl ProcessExplorerApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let mut system = System::new_all();
        system.refresh_all();
        system.refresh_cpu_all();

        let num_cpus = system.cpus().len().max(1); // Ensure at least 1 CPU
        
        Self {
            system,
            selected_pid: None,
            filter_text: String::new(),
            sort_column: SortColumn::default(),
            sort_ascending: true,
            refresh_interval: std::time::Duration::from_millis(1000),
            last_refresh: std::time::Instant::now(),
            visible_columns: VisibleColumns::default(),
            cpu_history: Vec::new(),
            memory_history: Vec::new(),
            max_history_points: 60, // Store 60 data points (1 minute at 1 second intervals)
            show_resource_window: false,
            resource_window: ResourceWindow {
                selected_tab: ResourceTab::default(),
                show_one_graph_per_cpu: true,
                cpu_core_histories: vec![Vec::new(); num_cpus],
                memory_history: Vec::new(),
                io_read_history: Vec::new(),
                io_write_history: Vec::new(),
            },
        }
    }

    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.system.refresh_processes(ProcessesToUpdate::All);
            self.system.refresh_cpu_all();
            self.system.refresh_memory();
            
            // Update historical data
            let cpu_usage = self.system.global_cpu_usage() as f64;
            let total_memory = self.system.total_memory();
            let used_memory = self.system.used_memory();
            let memory_percent = if total_memory > 0 {
                (used_memory as f64 / total_memory as f64) * 100.0
            } else {
                0.0
            };
            
            self.cpu_history.push(cpu_usage);
            self.memory_history.push(memory_percent);
            
            // Update per-CPU core histories
            let cpus = self.system.cpus();
            while self.resource_window.cpu_core_histories.len() < cpus.len() {
                self.resource_window.cpu_core_histories.push(Vec::new());
            }
            
            for (i, cpu) in cpus.iter().enumerate() {
                if i < self.resource_window.cpu_core_histories.len() {
                    self.resource_window.cpu_core_histories[i].push(cpu.cpu_usage() as f64);
                    if self.resource_window.cpu_core_histories[i].len() > self.max_history_points {
                        self.resource_window.cpu_core_histories[i].remove(0);
                    }
                }
            }
            
            // Update resource window memory history
            self.resource_window.memory_history.push(memory_percent);
            if self.resource_window.memory_history.len() > self.max_history_points {
                self.resource_window.memory_history.remove(0);
            }
            
            // Keep only the last N points
            if self.cpu_history.len() > self.max_history_points {
                self.cpu_history.remove(0);
            }
            if self.memory_history.len() > self.max_history_points {
                self.memory_history.remove(0);
            }
            
            self.last_refresh = std::time::Instant::now();
        }
    }

    fn get_process_list(&self) -> Vec<(Pid, &Process)> {
        let mut processes: Vec<(Pid, &Process)> = self.system.processes()
            .iter()
            .map(|(pid, proc)| (*pid, proc))
            .collect();

        // Apply filter
        if !self.filter_text.is_empty() {
            let filter_lower = self.filter_text.to_lowercase();
            processes.retain(|(pid, proc)| {
                proc.name().to_string_lossy().to_lowercase().contains(&filter_lower) ||
                pid.as_u32().to_string().contains(&self.filter_text)
            });
        }

        // Sort
        processes.sort_by(|(pid_a, proc_a), (pid_b, proc_b)| {
            let cmp = match self.sort_column {
                SortColumn::Name => proc_a.name().to_string_lossy().cmp(&proc_b.name().to_string_lossy()),
                SortColumn::Pid => pid_a.cmp(pid_b),
                SortColumn::Cpu => {
                    proc_a.cpu_usage().partial_cmp(&proc_b.cpu_usage())
                        .unwrap_or(std::cmp::Ordering::Equal)
                },
                SortColumn::Memory => {
                    proc_a.memory().cmp(&proc_b.memory())
                },
                SortColumn::Status => {
                    // Status is not directly available, use memory as proxy
                    // This is a placeholder - status sorting could be improved
                    proc_a.memory().cmp(&proc_b.memory())
                },
                SortColumn::VirtualMemory => {
                    proc_a.virtual_memory().cmp(&proc_b.virtual_memory())
                },
                SortColumn::ParentPid => {
                    proc_a.parent().cmp(&proc_b.parent())
                },
                SortColumn::StartTime => {
                    proc_a.start_time().cmp(&proc_b.start_time())
                },
                SortColumn::ExecutablePath => {
                    let exe_a = proc_a.exe().map(|p| p.display().to_string()).unwrap_or_default();
                    let exe_b = proc_b.exe().map(|p| p.display().to_string()).unwrap_or_default();
                    exe_a.cmp(&exe_b)
                },
                SortColumn::WorkingDirectory => {
                    let cwd_a = proc_a.cwd().map(|p| p.display().to_string()).unwrap_or_default();
                    let cwd_b = proc_b.cwd().map(|p| p.display().to_string()).unwrap_or_default();
                    cwd_a.cmp(&cwd_b)
                },
            };
            if self.sort_ascending {
                cmp
            } else {
                cmp.reverse()
            }
        });

        processes
    }

    fn format_bytes(&self, bytes: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;
        
        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }
        
        if unit_idx == 0 {
            format!("{} {}", bytes, UNITS[unit_idx])
        } else {
            format!("{:.2} {}", size, UNITS[unit_idx])
        }
    }

    fn show_mini_plot(&self, ui: &mut egui::Ui, history: &[f64], color: egui::Color32, max_value: f64, plot_id: &str) {
        if history.is_empty() {
            return;
        }

        let points: PlotPoints = history
            .iter()
            .enumerate()
            .map(|(i, &value)| [i as f64, value])
            .collect();

        let line = Line::new(points)
            .color(color)
            .width(1.5);

        Plot::new(plot_id)
            .height(20.0)
            .width(80.0)
            .show_axes([false, false])
            .show_grid([false, false])
            .show_background(false)
            .include_y(0.0)
            .include_y(max_value)
            .show(ui, |plot_ui| {
                plot_ui.line(line);
            });
    }

    fn show_resource_window_ui(&mut self, ui: &mut egui::Ui) {
        // Tabs
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.resource_window.selected_tab, ResourceTab::Summary, "Summary");
            ui.selectable_value(&mut self.resource_window.selected_tab, ResourceTab::Cpu, "CPU");
            ui.selectable_value(&mut self.resource_window.selected_tab, ResourceTab::Memory, "Memory");
            ui.selectable_value(&mut self.resource_window.selected_tab, ResourceTab::Io, "I/O");
        });
        ui.separator();

        match self.resource_window.selected_tab {
            ResourceTab::Summary => self.show_summary_tab(ui),
            ResourceTab::Cpu => self.show_cpu_tab(ui),
            ResourceTab::Memory => self.show_memory_tab(ui),
            ResourceTab::Io => self.show_io_tab(ui),
        }
    }

    fn show_summary_tab(&self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("System Summary");
            ui.separator();
            
            let total_memory = self.system.total_memory();
            let used_memory = self.system.used_memory();
            let total_swap = self.system.total_swap();
            let used_swap = self.system.used_swap();
            let process_count = self.system.processes().len();
            let cpu_usage = self.system.global_cpu_usage();
            
            ui.label(format!("CPU Usage: {:.2}%", cpu_usage));
            ui.label(format!("Memory: {} / {} ({:.2}%)",
                self.format_bytes(used_memory),
                self.format_bytes(total_memory),
                (used_memory as f64 / total_memory as f64) * 100.0));
            if total_swap > 0 {
                ui.label(format!("Swap: {} / {} ({:.2}%)",
                    self.format_bytes(used_swap),
                    self.format_bytes(total_swap),
                    (used_swap as f64 / total_swap as f64) * 100.0));
            }
            ui.label(format!("Processes: {}", process_count));
        });
    }

    fn show_cpu_tab(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            let cpu_usage = self.system.global_cpu_usage();
            let cpus = self.system.cpus();
            
            ui.horizontal(|ui| {
                // Overall CPU usage bar
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("CPU").size(16.0).strong());
                    let bar_height = 200.0;
                    let bar_width = 60.0;
                    
                    let (rect, _) = ui.allocate_at_least(
                        egui::vec2(bar_width, bar_height),
                        egui::Sense::hover()
                    );
                    
                    // Background
                    ui.painter().rect_filled(
                        rect,
                        0.0,
                        egui::Color32::from_rgb(40, 40, 40)
                    );
                    
                    // CPU usage fill (from bottom)
                    let fill_height = bar_height * (cpu_usage / 100.0);
                    let fill_rect = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x, rect.max.y - fill_height.max(0.0)),
                        rect.max
                    );
                    ui.painter().rect_filled(
                        fill_rect,
                        0.0,
                        egui::Color32::from_rgb(100, 200, 100)
                    );
                    
                    // Label
                    ui.label(format!("{:.2}%", cpu_usage));
                });
                
                ui.add_space(20.0);
                
                // CPU core graphs
                ui.vertical(|ui| {
                    ui.checkbox(&mut self.resource_window.show_one_graph_per_cpu, "Show one graph per CPU");
                    ui.separator();
                    
                    if self.resource_window.show_one_graph_per_cpu {
                        // Grid of CPU graphs
                        let num_cpus = cpus.len();
                        let cols = 8.min(num_cpus);
                        let rows = (num_cpus + cols - 1) / cols;
                        
                        egui::Grid::new("cpu_grid")
                            .num_columns(cols)
                            .spacing([4.0, 4.0])
                            .show(ui, |ui| {
                                for (i, cpu) in cpus.iter().enumerate() {
                                    ui.vertical(|ui| {
                                        ui.label(format!("CPU {}", i));
                                        
                                        let history: &[f64] = if i < self.resource_window.cpu_core_histories.len() {
                                            &self.resource_window.cpu_core_histories[i]
                                        } else {
                                            &[]
                                        };
                                        
                                        if !history.is_empty() {
                                            let points: PlotPoints = history
                                                .iter()
                                                .enumerate()
                                                .map(|(j, &value)| [j as f64, value])
                                                .collect();
                                            
                                            let line = Line::new(points)
                                                .color(egui::Color32::from_rgb(100, 200, 100))
                                                .width(1.0);
                                            
                                            Plot::new(format!("cpu_core_{}", i))
                                                .height(60.0)
                                                .width(80.0)
                                                .show_axes([false, false])
                                                .show_grid([false, false])
                                                .show_background(false)
                                                .include_y(0.0)
                                                .include_y(100.0)
                                                .show(ui, |plot_ui| {
                                                    plot_ui.line(line);
                                                });
                                        } else {
                                            ui.add_sized([80.0, 60.0], egui::Label::new("No data"));
                                        }
                                        
                                        ui.label(format!("{:.1}%", cpu.cpu_usage()));
                                    });
                                    
                                    if (i + 1) % cols == 0 {
                                        ui.end_row();
                                    }
                                }
                            });
                    } else {
                        // Single combined graph
                        if !self.cpu_history.is_empty() {
                            let points: PlotPoints = self.cpu_history
                                .iter()
                                .enumerate()
                                .map(|(i, &value)| [i as f64, value])
                                .collect();
                            
                            let line = Line::new(points)
                                .color(egui::Color32::from_rgb(100, 200, 100))
                                .width(2.0);
                            
                            Plot::new("cpu_combined")
                                .height(200.0)
                                .width(600.0)
                                .show_axes([true, true])
                                .show_grid([true, true])
                                .include_y(0.0)
                                .include_y(100.0)
                                .show(ui, |plot_ui| {
                                    plot_ui.line(line);
                                });
                        }
                    }
                });
            });
            
            ui.separator();
            
            // Statistics
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading("Totals");
                    ui.label(format!("Processes: {}", self.system.processes().len()));
                    // Thread count not directly available in sysinfo
                    ui.label("Threads: N/A");
                });
                
                ui.add_space(20.0);
                
                ui.vertical(|ui| {
                    ui.heading("CPU");
                    ui.label(format!("Logical Processors: {}", cpus.len()));
                    ui.label(format!("Cores: {}", cpus.len() / 2.max(1))); // Approximation
                    ui.label(format!("Sockets: 1")); // Approximation
                });
            });
        });
    }

    fn show_memory_tab(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            let total_memory = self.system.total_memory();
            let used_memory = self.system.used_memory();
            let total_swap = self.system.total_swap();
            let used_swap = self.system.used_swap();
            let memory_percent = if total_memory > 0 {
                (used_memory as f64 / total_memory as f64) * 100.0
            } else {
                0.0
            };
            
            ui.heading("Memory Usage");
            ui.separator();
            
            // Memory graph
            if !self.resource_window.memory_history.is_empty() {
                let points: PlotPoints = self.resource_window.memory_history
                    .iter()
                    .enumerate()
                    .map(|(i, &value)| [i as f64, value])
                    .collect();
                
                let line = Line::new(points)
                    .color(egui::Color32::from_rgb(100, 150, 255))
                    .width(2.0);
                
                Plot::new("memory_plot")
                    .height(300.0)
                    .width(700.0)
                    .show_axes([true, true])
                    .show_grid([true, true])
                    .include_y(0.0)
                    .include_y(100.0)
                    .show(ui, |plot_ui| {
                        plot_ui.line(line);
                    });
            }
            
            ui.separator();
            
            ui.label(format!("Total Memory: {}", self.format_bytes(total_memory)));
            ui.label(format!("Used Memory: {} ({:.2}%)",
                self.format_bytes(used_memory),
                memory_percent));
            ui.label(format!("Available Memory: {} ({:.2}%)",
                self.format_bytes(total_memory.saturating_sub(used_memory)),
                ((total_memory.saturating_sub(used_memory)) as f64 / total_memory as f64) * 100.0));
            
            if total_swap > 0 {
                ui.separator();
                ui.label(format!("Total Swap: {}", self.format_bytes(total_swap)));
                ui.label(format!("Used Swap: {} ({:.2}%)",
                    self.format_bytes(used_swap),
                    (used_swap as f64 / total_swap as f64) * 100.0));
            }
        });
    }

    fn show_io_tab(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.heading("I/O Statistics");
            ui.separator();
            ui.label("I/O statistics are not yet implemented.");
            ui.label("This would show disk read/write rates and network I/O.");
        });
    }
}

impl eframe::App for ProcessExplorerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.refresh_if_needed();

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Refresh").clicked() {
                        self.system.refresh_all();
                        self.last_refresh = std::time::Instant::now();
                    }
                    if ui.button("Exit").clicked() {
                        std::process::exit(0);
                    }
                });
                ui.menu_button("View", |ui| {
                    ui.label("Extra Columns:");
                    ui.separator();
                    ui.checkbox(&mut self.visible_columns.virtual_memory, "Virtual Memory");
                    ui.checkbox(&mut self.visible_columns.parent_pid, "Parent PID");
                    ui.checkbox(&mut self.visible_columns.start_time, "Start Time");
                    ui.checkbox(&mut self.visible_columns.executable_path, "Executable Path");
                    ui.checkbox(&mut self.visible_columns.working_directory, "Working Directory");
                    ui.separator();
                    if ui.button("System Information...").clicked() {
                        self.show_resource_window = true;
                    }
                });
                ui.menu_button("Process", |ui| {
                    if let Some(pid) = self.selected_pid {
                        if ui.button("Kill Process").clicked() {
                            // Note: Killing processes requires platform-specific code
                            // This is a placeholder - actual implementation would need
                            // platform-specific syscalls
                            eprintln!("Kill process {} (not implemented)", pid.as_u32());
                        }
                    }
                });
            });
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let cpu_usage = self.system.global_cpu_usage();
                let total_memory = self.system.total_memory();
                let used_memory = self.system.used_memory();
                let total_swap = self.system.total_swap();
                let used_swap = self.system.used_swap();
                let process_count = self.system.processes().len();
                let memory_percent = if total_memory > 0 {
                    (used_memory as f64 / total_memory as f64) * 100.0
                } else {
                    0.0
                };

                // CPU mini graph
                if !self.cpu_history.is_empty() {
                    self.show_mini_plot(
                        ui,
                        &self.cpu_history,
                        egui::Color32::from_rgb(100, 200, 100),
                        100.0,
                        "cpu_plot"
                    );
                    ui.add_space(4.0);
                }
                ui.label(format!("CPU: {:.2}%", cpu_usage));
                ui.separator();
                
                // Memory mini graph
                if !self.memory_history.is_empty() {
                    self.show_mini_plot(
                        ui,
                        &self.memory_history,
                        egui::Color32::from_rgb(100, 150, 255),
                        100.0,
                        "memory_plot"
                    );
                    ui.add_space(4.0);
                }
                ui.label(format!("Memory: {:.2}% ({})", 
                    memory_percent,
                    self.format_bytes(used_memory)));
                ui.separator();
                if total_swap > 0 {
                    ui.label(format!("Swap: {:.2}% ({})",
                        (used_swap as f64 / total_swap as f64) * 100.0,
                        self.format_bytes(used_swap)));
                    ui.separator();
                }
                ui.label(format!("Processes: {}", process_count));
            });
        });

        // Resource monitoring window
        let mut window_open = self.show_resource_window;
        egui::Window::new("System Information")
            .open(&mut window_open)
            .collapsible(false)
            .resizable(true)
            .default_size([800.0, 600.0])
            .show(ctx, |ui| {
                self.show_resource_window_ui(ui);
            });
        self.show_resource_window = window_open;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                // Toolbar with filter
                ui.horizontal(|ui| {
                    if ui.button("🔄 Refresh").clicked() {
                        self.system.refresh_all();
                        self.last_refresh = std::time::Instant::now();
                    }
                    ui.separator();
                    ui.label("Filter:");
                    ui.text_edit_singleline(&mut self.filter_text);
                });
                ui.separator();

                // Calculate available height for the panels
                let available_height = ui.available_height();
                let available_width = ui.available_width();

                // Split view: Process list and details
                ui.horizontal(|ui| {
                    // Left panel: Process list (60% width)
                    ui.allocate_ui_with_layout(
                        egui::vec2(available_width * 0.6, available_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.heading("Processes");

                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .id_source("process_list_scroll")
                                .show(ui, |ui| {
                                    // Calculate number of columns
                                    let mut num_columns = 5; // Default columns: Process, PID, CPU, Memory, Status
                                    if self.visible_columns.virtual_memory { num_columns += 1; }
                                    if self.visible_columns.parent_pid { num_columns += 1; }
                                    if self.visible_columns.start_time { num_columns += 1; }
                                    if self.visible_columns.executable_path { num_columns += 1; }
                                    if self.visible_columns.working_directory { num_columns += 1; }

                                    egui::Grid::new("process_grid")
                                        .num_columns(num_columns)
                                        .spacing([4.0, 2.0])
                                        .striped(true)
                                        .show(ui, |ui| {
                                            // Header row
                                            ui.strong("Process");
                                            if ui.selectable_label(
                                                self.sort_column == SortColumn::Pid,
                                                "PID"
                                            ).clicked() {
                                                if self.sort_column == SortColumn::Pid {
                                                    self.sort_ascending = !self.sort_ascending;
                                                } else {
                                                    self.sort_column = SortColumn::Pid;
                                                    self.sort_ascending = true;
                                                }
                                            }
                                            if ui.selectable_label(
                                                self.sort_column == SortColumn::Cpu,
                                                "CPU %"
                                            ).clicked() {
                                                if self.sort_column == SortColumn::Cpu {
                                                    self.sort_ascending = !self.sort_ascending;
                                                } else {
                                                    self.sort_column = SortColumn::Cpu;
                                                    self.sort_ascending = false; // Default: high CPU first
                                                }
                                            }
                                            if ui.selectable_label(
                                                self.sort_column == SortColumn::Memory,
                                                "Memory"
                                            ).clicked() {
                                                if self.sort_column == SortColumn::Memory {
                                                    self.sort_ascending = !self.sort_ascending;
                                                } else {
                                                    self.sort_column = SortColumn::Memory;
                                                    self.sort_ascending = false; // Default: high memory first
                                                }
                                            }
                                            ui.strong("Status");

                                            // Extra column headers
                                            if self.visible_columns.virtual_memory {
                                                if ui.selectable_label(
                                                    self.sort_column == SortColumn::VirtualMemory,
                                                    "Virtual Mem"
                                                ).clicked() {
                                                    if self.sort_column == SortColumn::VirtualMemory {
                                                        self.sort_ascending = !self.sort_ascending;
                                                    } else {
                                                        self.sort_column = SortColumn::VirtualMemory;
                                                        self.sort_ascending = false;
                                                    }
                                                }
                                            }
                                            if self.visible_columns.parent_pid {
                                                if ui.selectable_label(
                                                    self.sort_column == SortColumn::ParentPid,
                                                    "Parent PID"
                                                ).clicked() {
                                                    if self.sort_column == SortColumn::ParentPid {
                                                        self.sort_ascending = !self.sort_ascending;
                                                    } else {
                                                        self.sort_column = SortColumn::ParentPid;
                                                        self.sort_ascending = true;
                                                    }
                                                }
                                            }
                                            if self.visible_columns.start_time {
                                                if ui.selectable_label(
                                                    self.sort_column == SortColumn::StartTime,
                                                    "Start Time"
                                                ).clicked() {
                                                    if self.sort_column == SortColumn::StartTime {
                                                        self.sort_ascending = !self.sort_ascending;
                                                    } else {
                                                        self.sort_column = SortColumn::StartTime;
                                                        self.sort_ascending = true;
                                                    }
                                                }
                                            }
                                            if self.visible_columns.executable_path {
                                                if ui.selectable_label(
                                                    self.sort_column == SortColumn::ExecutablePath,
                                                    "Executable"
                                                ).clicked() {
                                                    if self.sort_column == SortColumn::ExecutablePath {
                                                        self.sort_ascending = !self.sort_ascending;
                                                    } else {
                                                        self.sort_column = SortColumn::ExecutablePath;
                                                        self.sort_ascending = true;
                                                    }
                                                }
                                            }
                                            if self.visible_columns.working_directory {
                                                if ui.selectable_label(
                                                    self.sort_column == SortColumn::WorkingDirectory,
                                                    "Working Dir"
                                                ).clicked() {
                                                    if self.sort_column == SortColumn::WorkingDirectory {
                                                        self.sort_ascending = !self.sort_ascending;
                                                    } else {
                                                        self.sort_column = SortColumn::WorkingDirectory;
                                                        self.sort_ascending = true;
                                                    }
                                                }
                                            }
                                            ui.end_row();

                                            // Process rows
                                            let selected_pid = self.selected_pid;
                                            let processes = self.get_process_list();
                                            let mut clicked_pid: Option<Pid> = None;
                                            
                                            for (pid, process) in processes {
                                                let is_selected = selected_pid == Some(pid);
                                                
                                                // Color coding based on process state
                                                let mut text_color = egui::Color32::WHITE;
                                                if process.cpu_usage() > 50.0 {
                                                    text_color = egui::Color32::from_rgb(255, 100, 100);
                                                } else if process.cpu_usage() > 10.0 {
                                                    text_color = egui::Color32::from_rgb(255, 200, 100);
                                                }

                                                let process_name = process.name().to_string_lossy();
                                                let response = ui.selectable_label(
                                                    is_selected,
                                                    process_name.as_ref()
                                                )
                                                .on_hover_text(process.exe().map(|p| p.display().to_string())
                                                    .unwrap_or_else(|| "Unknown path".to_string()));
                                                
                                                if response.clicked() {
                                                    clicked_pid = Some(pid);
                                                }
                                                
                                                if is_selected {
                                                    response.highlight();
                                                }

                                                ui.label(egui::RichText::new(format!("{}", pid.as_u32()))
                                                    .color(text_color));
                                                ui.label(egui::RichText::new(format!("{:.2}", process.cpu_usage()))
                                                    .color(text_color));
                                                ui.label(egui::RichText::new(self.format_bytes(process.memory()))
                                                    .color(text_color));

                                                let status = format!("{:?}", process.status());
                                                ui.label(status);

                                                // Extra column data
                                                if self.visible_columns.virtual_memory {
                                                    ui.label(egui::RichText::new(self.format_bytes(process.virtual_memory()))
                                                        .color(text_color));
                                                }
                                                if self.visible_columns.parent_pid {
                                                    let parent_str = process.parent()
                                                        .map(|p| p.as_u32().to_string())
                                                        .unwrap_or_else(|| "-".to_string());
                                                    ui.label(egui::RichText::new(parent_str)
                                                        .color(text_color));
                                                }
                                                if self.visible_columns.start_time {
                                                    ui.label(egui::RichText::new(format!("{}", process.start_time()))
                                                        .color(text_color));
                                                }
                                                if self.visible_columns.executable_path {
                                                    let exe_str = process.exe()
                                                        .map(|p| p.display().to_string())
                                                        .unwrap_or_else(|| "-".to_string());
                                                    ui.label(egui::RichText::new(exe_str)
                                                        .color(text_color)
                                                        .size(10.0));
                                                }
                                                if self.visible_columns.working_directory {
                                                    let cwd_str = process.cwd()
                                                        .map(|p| p.display().to_string())
                                                        .unwrap_or_else(|| "-".to_string());
                                                    ui.label(egui::RichText::new(cwd_str)
                                                        .color(text_color)
                                                        .size(10.0));
                                                }

                                                ui.end_row();
                                            }
                                            
                                            // Update selected PID after the loop
                                            if let Some(pid) = clicked_pid {
                                                self.selected_pid = Some(pid);
                                            }
                                        });
                                });
                        }
                    );

                    ui.separator();

                    // Right panel: Process details (40% width)
                    ui.allocate_ui_with_layout(
                        egui::vec2(available_width * 0.4 - 20.0, available_height),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.heading("Process Details");

                            if let Some(pid) = self.selected_pid {
                                if let Some(process) = self.system.process(pid) {
                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            ui.group(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new("General").strong().heading());
                                                    ui.separator();
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("Name: ");
                                                        ui.label(process.name().to_string_lossy());
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("PID: ");
                                                        ui.label(pid.as_u32().to_string());
                                                    });
                                                    
                                                    if let Some(exe) = process.exe() {
                                                        ui.horizontal(|ui| {
                                                            ui.strong("Path: ");
                                                            ui.label(exe.display().to_string());
                                                        });
                                                    }
                                                    
                                                    if let Some(cwd) = process.cwd() {
                                                        ui.horizontal(|ui| {
                                                            ui.strong("Working Directory: ");
                                                            ui.label(cwd.display().to_string());
                                                        });
                                                    }
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("CPU Usage: ");
                                                        ui.label(format!("{:.2}%", process.cpu_usage()));
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("Memory: ");
                                                        ui.label(self.format_bytes(process.memory()));
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("Virtual Memory: ");
                                                        ui.label(self.format_bytes(process.virtual_memory()));
                                                    });
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("Status: ");
                                                        ui.label(format!("{:?}", process.status()));
                                                    });
                                                    
                                                    if let Some(parent) = process.parent() {
                                                        ui.horizontal(|ui| {
                                                            ui.strong("Parent PID: ");
                                                            ui.label(parent.as_u32().to_string());
                                                        });
                                                    }
                                                    
                                                    ui.horizontal(|ui| {
                                                        ui.strong("Start Time: ");
                                                        let start_time = process.start_time();
                                                        ui.label(format!("{}", start_time));
                                                    });
                                                });
                                            });
                                            
                                            ui.add_space(10.0);
                                            
                                            // Command line / Arguments
                                            ui.group(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new("Command Line").strong().heading());
                                                    ui.separator();
                                                    
                                                    let cmd = process.cmd();
                                                    if cmd.is_empty() {
                                                        ui.label("(No command line available)");
                                                    } else {
                                                        for arg in cmd {
                                                            ui.label(format!("  {}", arg.to_string_lossy()));
                                                        }
                                                    }
                                                });
                                            });
                                            
                                            ui.add_space(10.0);
                                            
                                            // Environment variables (limited display)
                                            ui.group(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new("Environment Variables").strong().heading());
                                                    ui.separator();
                                                    
                                                    // Note: sysinfo doesn't provide direct access to env vars
                                                    // This is a placeholder
                                                    ui.label("(Environment variables not available via sysinfo)");
                                                });
                                            });
                                            
                                            ui.add_space(10.0);
                                            
                                            // Open files / Handles
                                            ui.group(|ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(egui::RichText::new("Open Files").strong().heading());
                                                    ui.separator();
                                                    
                                                    // Note: sysinfo doesn't provide direct access to open files
                                                    // This would require platform-specific code
                                                    ui.label("(Open files not available via sysinfo)");
                                                });
                                            });
                                        });
                                } else {
                                    ui.label("Process no longer exists");
                                    self.selected_pid = None;
                                }
                            } else {
                                ui.label("Select a process to view details");
                            }
                        }
                    );
                });
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("Process Explorer"),
        ..Default::default()
    };
    
    eframe::run_native(
        "Process Explorer",
        options,
        Box::new(|cc| Box::new(ProcessExplorerApp::new(cc))),
    )
}
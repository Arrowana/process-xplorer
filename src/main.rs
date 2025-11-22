use eframe::egui;
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

        Self {
            system,
            selected_pid: None,
            filter_text: String::new(),
            sort_column: SortColumn::default(),
            sort_ascending: true,
            refresh_interval: std::time::Duration::from_millis(1000),
            last_refresh: std::time::Instant::now(),
            visible_columns: VisibleColumns::default(),
        }
    }

    fn refresh_if_needed(&mut self) {
        if self.last_refresh.elapsed() >= self.refresh_interval {
            self.system.refresh_processes(ProcessesToUpdate::All);
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

                ui.label(format!("CPU: {:.2}%", cpu_usage));
                ui.separator();
                ui.label(format!("Memory: {:.2}% ({})", 
                    (used_memory as f64 / total_memory as f64) * 100.0,
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
//! UI rendering for the TUI dashboard.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table},
    Frame,
};

use super::app::{format_uptime, App, MessageType, ServiceStatus};

/// Draw the entire UI.
pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Service table
            Constraint::Length(4), // Footer
        ])
        .split(frame.area());

    draw_header(frame, chunks[0], app);
    draw_service_table(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);

    // Draw overlays
    if app.show_detail {
        if let Some(service) = app.selected_service() {
            draw_detail_overlay(frame, service, &app.detail_methods);
        }
    } else if app.show_help {
        draw_help_overlay(frame);
    }
}

/// Draw the header with title and last update time.
fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = app.last_refresh.elapsed().as_secs();
    let time_str = if elapsed < 60 {
        format!("{}s ago", elapsed)
    } else {
        format!("{}m ago", elapsed / 60)
    };

    let title = Line::from(vec![
        Span::styled(
            " FGP Dashboard ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("                                        "),
        Span::styled(
            format!("Updated: {} ", time_str),
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(title);

    frame.render_widget(block, area);
}

/// Draw the service table.
fn draw_service_table(frame: &mut Frame, area: Rect, app: &App) {
    let header_cells = ["", "Service", "Status", "Version", "Uptime"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header = Row::new(header_cells).height(1);

    let rows: Vec<Row> = app
        .services
        .iter()
        .enumerate()
        .map(|(i, service)| {
            let selected = i == app.selected;

            // Selection indicator
            let selector = if selected { "▸" } else { " " };
            let selector_style = if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };

            // Status styling
            let (status_color, status_text) = match service.status {
                ServiceStatus::Running => {
                    (Color::Green, format!("{} running", service.status.symbol()))
                }
                ServiceStatus::Stopped => (
                    Color::DarkGray,
                    format!("{} stopped", service.status.symbol()),
                ),
                ServiceStatus::Unhealthy => (
                    Color::Yellow,
                    format!("{} unhealthy", service.status.symbol()),
                ),
                ServiceStatus::Error => (Color::Red, format!("{} error", service.status.symbol())),
                ServiceStatus::Starting => {
                    (Color::Blue, format!("{} starting", service.status.symbol()))
                }
                ServiceStatus::Stopping => {
                    (Color::Blue, format!("{} stopping", service.status.symbol()))
                }
            };

            // Version and uptime
            let version = service.version.as_deref().unwrap_or("-");
            let uptime = service
                .uptime_seconds
                .map(format_uptime)
                .unwrap_or_else(|| "-".to_string());

            // Row styling
            let row_style = if selected {
                Style::default().bg(Color::DarkGray)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(selector).style(selector_style),
                Cell::from(service.name.clone()),
                Cell::from(status_text).style(Style::default().fg(status_color)),
                Cell::from(version.to_string()),
                Cell::from(uptime),
            ])
            .style(row_style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),  // Selector
        Constraint::Min(15),    // Service name
        Constraint::Length(14), // Status
        Constraint::Length(10), // Version
        Constraint::Length(10), // Uptime
    ];

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    format!(" Services ({}) ", app.services.len()),
                    Style::default().fg(Color::White),
                )),
        )
        .row_highlight_style(Style::default());

    frame.render_widget(table, area);
}

/// Draw the footer with keybindings and messages.
fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Length(2)])
        .split(area);

    // Keybindings
    let keybindings = Line::from(vec![
        Span::styled(" [↑/k]", Style::default().fg(Color::Yellow)),
        Span::raw(" Up  "),
        Span::styled("[↓/j]", Style::default().fg(Color::Yellow)),
        Span::raw(" Down  "),
        Span::styled("[s]", Style::default().fg(Color::Green)),
        Span::raw(" Start  "),
        Span::styled("[x]", Style::default().fg(Color::Red)),
        Span::raw(" Stop  "),
        Span::styled("[R]", Style::default().fg(Color::Blue)),
        Span::raw(" Restart  "),
        Span::styled("[d]", Style::default().fg(Color::Cyan)),
        Span::raw(" Detail  "),
        Span::styled("[?]", Style::default().fg(Color::Magenta)),
        Span::raw(" Help  "),
        Span::styled("[q]", Style::default().fg(Color::DarkGray)),
        Span::raw(" Quit"),
    ]);

    let keybindings_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let keybindings_paragraph = Paragraph::new(keybindings).block(keybindings_block);
    frame.render_widget(keybindings_paragraph, chunks[0]);

    // Message area
    let message_line = if let Some((text, msg_type, _)) = &app.message {
        let (symbol, color) = match msg_type {
            MessageType::Success => ("✓", Color::Green),
            MessageType::Error => ("✗", Color::Red),
        };
        Line::from(vec![
            Span::styled(format!(" {} ", symbol), Style::default().fg(color)),
            Span::styled(text.clone(), Style::default().fg(color)),
        ])
    } else {
        Line::from("")
    };

    let message_block = Block::default()
        .borders(Borders::BOTTOM | Borders::LEFT | Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray));

    let message_paragraph = Paragraph::new(message_line).block(message_block);
    frame.render_widget(message_paragraph, chunks[1]);
}

/// Draw the help overlay.
fn draw_help_overlay(frame: &mut Frame) {
    let area = centered_rect(50, 60, frame.area());

    // Clear the area first
    frame.render_widget(Clear, area);

    let help_text = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "  Keyboard Shortcuts",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ↑/k      ", Style::default().fg(Color::Yellow)),
            Span::raw("Select previous service"),
        ]),
        Line::from(vec![
            Span::styled("  ↓/j      ", Style::default().fg(Color::Yellow)),
            Span::raw("Select next service"),
        ]),
        Line::from(vec![
            Span::styled("  Home     ", Style::default().fg(Color::Yellow)),
            Span::raw("Select first service"),
        ]),
        Line::from(vec![
            Span::styled("  End      ", Style::default().fg(Color::Yellow)),
            Span::raw("Select last service"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  s        ", Style::default().fg(Color::Green)),
            Span::raw("Start selected service"),
        ]),
        Line::from(vec![
            Span::styled("  x        ", Style::default().fg(Color::Red)),
            Span::raw("Stop selected service"),
        ]),
        Line::from(vec![
            Span::styled("  R        ", Style::default().fg(Color::Blue)),
            Span::raw("Restart selected service"),
        ]),
        Line::from(vec![
            Span::styled("  d/Enter  ", Style::default().fg(Color::Cyan)),
            Span::raw("View service details"),
        ]),
        Line::from(vec![
            Span::styled("  r        ", Style::default().fg(Color::Cyan)),
            Span::raw("Refresh service list"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ?        ", Style::default().fg(Color::Magenta)),
            Span::raw("Toggle this help"),
        ]),
        Line::from(vec![
            Span::styled("  q/Esc    ", Style::default().fg(Color::DarkGray)),
            Span::raw("Quit"),
        ]),
        Line::from(""),
    ];

    let help_block = Block::default()
        .title(Span::styled(
            " Help ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Black));

    let help_paragraph = Paragraph::new(help_text).block(help_block);
    frame.render_widget(help_paragraph, area);
}

/// Draw the service detail overlay.
fn draw_detail_overlay(frame: &mut Frame, service: &super::app::ServiceInfo, methods: &[String]) {
    let area = centered_rect(60, 70, frame.area());

    // Clear the area first
    frame.render_widget(Clear, area);

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            format!("  {}", service.name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
    ];

    // Status
    let (status_color, status_text) = match service.status {
        super::app::ServiceStatus::Running => (Color::Green, "● running"),
        super::app::ServiceStatus::Stopped => (Color::DarkGray, "○ stopped"),
        super::app::ServiceStatus::Unhealthy => (Color::Yellow, "◐ unhealthy"),
        super::app::ServiceStatus::Error => (Color::Red, "● error"),
        super::app::ServiceStatus::Starting => (Color::Blue, "◑ starting"),
        super::app::ServiceStatus::Stopping => (Color::Blue, "◑ stopping"),
    };

    lines.push(Line::from(vec![
        Span::raw("  Status:   "),
        Span::styled(status_text, Style::default().fg(status_color)),
    ]));

    // Version
    if let Some(ref version) = service.version {
        lines.push(Line::from(vec![
            Span::raw("  Version:  "),
            Span::styled(version.clone(), Style::default().fg(Color::White)),
        ]));
    }

    // Uptime
    if let Some(uptime) = service.uptime_seconds {
        lines.push(Line::from(vec![
            Span::raw("  Uptime:   "),
            Span::styled(
                super::app::format_uptime(uptime),
                Style::default().fg(Color::White),
            ),
        ]));
    }

    lines.push(Line::from(""));

    // Methods
    if !methods.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            format!("  Available Methods ({}):", methods.len()),
            Style::default().fg(Color::Yellow),
        )]));

        for method in methods.iter().take(15) {
            lines.push(Line::from(vec![
                Span::raw("    • "),
                Span::styled(method.clone(), Style::default().fg(Color::Green)),
            ]));
        }

        if methods.len() > 15 {
            lines.push(Line::from(vec![Span::styled(
                format!("    ... and {} more", methods.len() - 15),
                Style::default().fg(Color::DarkGray),
            )]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![Span::styled(
        "  Press Esc or d to close",
        Style::default().fg(Color::DarkGray),
    )]));

    let detail_block = Block::default()
        .title(Span::styled(
            " Service Details ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue))
        .style(Style::default().bg(Color::Black));

    let detail_paragraph = Paragraph::new(lines).block(detail_block);
    frame.render_widget(detail_paragraph, area);
}

/// Create a centered rectangle.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

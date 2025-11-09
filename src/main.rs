use anyhow::Result;
use chrono::Local;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use rusqlite::{params, Connection};
use std::io;

#[derive(Debug, Clone)]
struct WorkoutRecord {
    exercise_type: String,
    count: i32,
    timestamp: String,
}

struct Database {
    conn: Connection,
}

impl Database {
    fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workouts (
                id INTEGER PRIMARY KEY,
                exercise_type TEXT NOT NULL,
                count INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { conn })
    }

    fn add_workout(&self, exercise_type: &str, count: i32) -> Result<()> {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.conn.execute(
            "INSERT INTO workouts (exercise_type, count, timestamp) VALUES (?1, ?2, ?3)",
            params![exercise_type, count, timestamp],
        )?;
        Ok(())
    }

    fn get_today_workouts(&self) -> Result<Vec<WorkoutRecord>> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let mut stmt = self.conn.prepare(
            "SELECT exercise_type, count, timestamp FROM workouts 
             WHERE date(timestamp) = date(?1) 
             ORDER BY timestamp DESC",
        )?;
        
        let records = stmt
            .query_map([today], |row| {
                Ok(WorkoutRecord {
                    exercise_type: row.get(0)?,
                    count: row.get(1)?,
                    timestamp: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(records)
    }

    fn get_workouts_by_date(&self, date: &str) -> Result<Vec<WorkoutRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT exercise_type, count, timestamp FROM workouts 
             WHERE date(timestamp) = date(?1) 
             ORDER BY timestamp DESC",
        )?;
        
        let records = stmt
            .query_map([date], |row| {
                Ok(WorkoutRecord {
                    exercise_type: row.get(0)?,
                    count: row.get(1)?,
                    timestamp: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(records)
    }

    fn get_unique_dates(&self) -> Result<Vec<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT date(timestamp) as workout_date 
             FROM workouts 
             ORDER BY workout_date DESC",
        )?;
        
        let dates = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(dates)
    }
}

enum Screen {
    Main,
    AddWorkout,
    History,
}

enum ExerciseType {
    Squats,
    PushUps,
}

struct App {
    db: Database,
    screen: Screen,
    selected_exercise: ExerciseType,
    input_count: String,
    history_selected: usize,
    selected_date: Option<String>,
    message: Option<String>,
}

impl App {
    fn new(db: Database) -> Self {
        Self {
            db,
            screen: Screen::Main,
            selected_exercise: ExerciseType::Squats,
            input_count: String::new(),
            history_selected: 0,
            selected_date: None,
            message: None,
        }
    }

    fn handle_input(&mut self, key: KeyCode) -> Result<bool> {
        match &self.screen {
            Screen::Main => self.handle_main_input(key),
            Screen::AddWorkout => self.handle_add_workout_input(key),
            Screen::History => self.handle_history_input(key),
        }
    }

    fn handle_main_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('a') => {
                self.screen = Screen::AddWorkout;
                self.input_count.clear();
                self.message = None;
            }
            KeyCode::Char('h') => {
                self.screen = Screen::History;
                self.history_selected = 0;
                self.selected_date = None;
                self.message = None;
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_add_workout_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => {
                self.screen = Screen::Main;
                self.input_count.clear();
            }
            KeyCode::Tab => {
                self.selected_exercise = match self.selected_exercise {
                    ExerciseType::Squats => ExerciseType::PushUps,
                    ExerciseType::PushUps => ExerciseType::Squats,
                };
            }
            KeyCode::Char(c) if c.is_ascii_digit() => {
                self.input_count.push(c);
            }
            KeyCode::Backspace => {
                self.input_count.pop();
            }
            KeyCode::Enter => {
                if let Ok(count) = self.input_count.parse::<i32>() {
                    if count > 0 {
                        let exercise = match self.selected_exercise {
                            ExerciseType::Squats => "squats",
                            ExerciseType::PushUps => "push-ups",
                        };
                        self.db.add_workout(exercise, count)?;
                        self.message = Some(format!("Added {} {}!", count, exercise));
                        self.input_count.clear();
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }

    fn handle_history_input(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => {
                if self.selected_date.is_some() {
                    self.selected_date = None;
                } else {
                    self.screen = Screen::Main;
                }
            }
            KeyCode::Up => {
                if self.selected_date.is_none() && self.history_selected > 0 {
                    self.history_selected -= 1;
                }
            }
            KeyCode::Down => {
                if self.selected_date.is_none() {
                    let dates = self.db.get_unique_dates()?;
                    if self.history_selected < dates.len().saturating_sub(1) {
                        self.history_selected += 1;
                    }
                }
            }
            KeyCode::Enter => {
                if self.selected_date.is_none() {
                    let dates = self.db.get_unique_dates()?;
                    if let Some(date) = dates.get(self.history_selected) {
                        self.selected_date = Some(date.clone());
                    }
                }
            }
            _ => {}
        }
        Ok(false)
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(f.size());

    match app.screen {
        Screen::Main => render_main_screen(f, chunks[0], app),
        Screen::AddWorkout => render_add_workout_screen(f, chunks[0], app),
        Screen::History => render_history_screen(f, chunks[0], app),
    }

    render_help(f, chunks[1], &app.screen);
}

fn render_main_screen(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Title
    let title = Paragraph::new("🏋️  Fitness Tracker")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Welcome"));
    f.render_widget(title, chunks[0]);

    // Today's summary
    if let Ok(workouts) = app.db.get_today_workouts() {
        let mut squats_total = 0;
        let mut pushups_total = 0;

        for workout in &workouts {
            match workout.exercise_type.as_str() {
                "squats" => squats_total += workout.count,
                "push-ups" => pushups_total += workout.count,
                _ => {}
            }
        }

        let summary = vec![
            Line::from(vec![
                Span::styled("Today's Stats:", Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::raw("Squats: "),
                Span::styled(
                    squats_total.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("Push-ups: "),
                Span::styled(
                    pushups_total.to_string(),
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    format!("Total workouts: {}", workouts.len()),
                    Style::default().fg(Color::Yellow),
                ),
            ]),
        ];

        let summary_widget = Paragraph::new(summary)
            .block(Block::default().borders(Borders::ALL).title("Today"))
            .wrap(Wrap { trim: true });
        f.render_widget(summary_widget, chunks[1]);
    }
}

fn render_add_workout_screen(f: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);

    // Exercise type selector
    let exercise_text = match app.selected_exercise {
        ExerciseType::Squats => "Squats (Tab to switch)",
        ExerciseType::PushUps => "Push-ups (Tab to switch)",
    };
    
    let exercise = Paragraph::new(exercise_text)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL).title("Exercise Type"));
    f.render_widget(exercise, chunks[0]);

    // Count input
    let input = Paragraph::new(app.input_count.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL).title("Count (Enter to save)"));
    f.render_widget(input, chunks[1]);

    // Message
    if let Some(msg) = &app.message {
        let message = Paragraph::new(msg.as_str())
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::ALL).title("Status"));
        f.render_widget(message, chunks[2]);
    }
}

fn render_history_screen(f: &mut Frame, area: Rect, app: &App) {
    if let Some(date) = &app.selected_date {
        // Show workouts for selected date
        if let Ok(workouts) = app.db.get_workouts_by_date(date) {
            let items: Vec<ListItem> = workouts
                .iter()
                .map(|w| {
                    let time = w.timestamp.split(' ').nth(1).unwrap_or("");
                    let content = format!("{} - {} {}", time, w.count, w.exercise_type);
                    ListItem::new(content)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title(format!("Workouts on {}", date)))
                .style(Style::default().fg(Color::White));
            f.render_widget(list, area);
        }
    } else {
        // Show date list
        if let Ok(dates) = app.db.get_unique_dates() {
            let items: Vec<ListItem> = dates
                .iter()
                .enumerate()
                .map(|(i, date)| {
                    let style = if i == app.history_selected {
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    ListItem::new(date.as_str()).style(style)
                })
                .collect();

            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Workout History (Enter to view)"))
                .style(Style::default().fg(Color::White));
            f.render_widget(list, area);
        }
    }
}

fn render_help(f: &mut Frame, area: Rect, screen: &Screen) {
    let help_text = match screen {
        Screen::Main => "[a] Add Workout  [h] History  [q] Quit",
        Screen::AddWorkout => "[Tab] Switch Exercise  [Enter] Save  [Esc] Back",
        Screen::History => "[↑/↓] Navigate  [Enter] Select  [Esc] Back",
    };

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, area);
}

fn main() -> Result<()> {
    // Setup database
    let db = Database::new("fitness_tracker.db")?;
    let mut app = App::new(db);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    loop {
        terminal.draw(|f| ui(f, &app))?;

        if let Event::Key(key) = event::read()? {
            if app.handle_input(key.code)? {
                break;
            }
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

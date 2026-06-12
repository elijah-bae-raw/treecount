use clap::Parser;
use colored::Colorize;
use core::fmt::Display;
use std::collections::BTreeMap;
use std::ops::{Add, AddAssign};
use std::path::{Path, PathBuf};
use tokei::{Config, LanguageType, Languages};

#[derive(Parser)]
#[command(name = "treecount", version, about = "Directory tree with lines of code")]
struct Cli {
    /// The path(s) to the file or directory to be counted.
    #[arg(default_value = ".")]
    paths: Vec<String>,

    /// Ignore all files & directories matching the pattern.
    #[arg(short, long, action = clap::ArgAction::Append)]
    exclude: Vec<String>,

    /// Count hidden files.
    #[arg(long)]
    hidden: bool,

    /// Don't respect ignore files (.gitignore, .ignore, etc.).
    /// Implies --no-ignore-parent, --no-ignore-dot, and --no-ignore-vcs.
    #[arg(long)]
    no_ignore: bool,

    /// Don't respect ignore files in parent directories.
    #[arg(long)]
    no_ignore_parent: bool,

    /// Don't respect .ignore and .tokeignore files.
    #[arg(long)]
    no_ignore_dot: bool,

    /// Don't respect VCS ignore files (.gitignore, .hgignore, etc.).
    #[arg(long)]
    no_ignore_vcs: bool,

    /// Filter by language type, separated by commas (e.g. -t Rust,Markdown).
    #[arg(short = 't', long = "types", action = clap::ArgAction::Append)]
    types: Vec<String>,

    /// Directory mode: only show directories, not individual files.
    #[arg(short = 'd')]
    dir_mode: bool,

    /// In directory mode, only count files directly in each directory (not recursive).
    #[arg(long)]
    direct: bool,

    /// Depth limit for directory mode (like tree -L).
    #[arg(short = 'L')]
    depth: Option<usize>,
}

#[derive(Clone, Copy)]
struct Count {
    code: usize,
    comments: usize,
    literate: usize,
}

impl Add for Count {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        return Self {
            code: self.code + rhs.code,
            comments: self.comments + rhs.comments,
            literate: self.literate + rhs.literate,
        };
    }
}

impl AddAssign<Count> for Count {
    fn add_assign(&mut self, rhs: Count) {
        self.comments += rhs.comments;
        self.code += rhs.code;
        self.literate += rhs.literate;
    }
}

impl Display for Count {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.total() == 0 {
            return Ok(());
        }

        write!(f, "({}", self.code.to_string().green())?;

        if self.comments > 0 {
            write!(f, " [//: {}]", self.comments.to_string().yellow())?;
        }

        if self.literate > 0 {
            write!(f, " [doc: {}]", self.literate.to_string().cyan())?;
        }

        write!(f, ")")
    }
}

impl Count {
    fn total(&self) -> usize {
        self.code + self.comments + self.literate
    }

    fn new() -> Self {
        Self {
            code: 0,
            comments: 0,
            literate: 0,
        }
    }
}

impl Default for Count {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
struct DirNode {
    name: String,
    files: Vec<(String, Count)>,
    dirs: BTreeMap<String, DirNode>,
    direct_count: Count,
    total_count: Count,
}

impl DirNode {
    fn new(name: String) -> Self {
        DirNode {
            name,
            files: Vec::new(),
            dirs: BTreeMap::new(),
            direct_count: Count::new(),
            total_count: Count::new(),
        }
    }

    fn insert_file(&mut self, components: &[&str], file_count: Count) {
        if components.len() == 1 {
            self.files.push((components[0].to_string(), file_count));
            self.direct_count = self.direct_count + file_count;
        } else {
            let dir_name = components[0];
            let child = self
                .dirs
                .entry(dir_name.to_string())
                .or_insert_with(|| DirNode::new(dir_name.to_string()));
            child.insert_file(&components[1..], file_count);
        }
    }

    fn compute_totals(&mut self) {
        self.total_count = self.direct_count;
        for child in self.dirs.values_mut() {
            child.compute_totals();
            self.total_count = self.total_count + child.total_count;
        }
    }
}

fn render_file_mode(node: &DirNode, prefix: &str) {
    let mut entries: Vec<Entry> = Vec::new();
    for (dir_name, dir_node) in &node.dirs {
        entries.push(Entry::Dir(dir_name.clone(), dir_node));
    }
    for (filename, code) in &node.files {
        entries.push(Entry::File(filename.clone(), code.clone()))
    }

    let count = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        match entry {
            Entry::Dir(name, dir_node) => {
                println!(
                    "{}{}{} {}",
                    prefix,
                    connector,
                    name.bold().blue(),
                    dir_node.total_count
                );
                render_file_mode(dir_node, &child_prefix);
            }
            Entry::File(name, count) => {
                println!("{}{}{} {}", prefix, connector, name, count);
            }
        }
    }
}

fn render_dir_mode(
    node: &DirNode,
    prefix: &str,
    direct: bool,
    depth: Option<usize>,
    current_depth: usize,
) {
    if let Some(max) = depth {
        if current_depth >= max {
            return;
        }
    }

    let dirs: Vec<(&String, &DirNode)> = node.dirs.iter().collect();
    let count = dirs.len();
    for (i, (dir_name, dir_node)) in dirs.iter().enumerate() {
        let is_last = i == count - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        let count = if direct {
            dir_node.direct_count
        } else {
            dir_node.total_count
        };

        println!(
            "{}{}{} {}",
            prefix,
            connector,
            dir_name.bold().blue(),
            count
        );
        render_dir_mode(dir_node, &child_prefix, direct, depth, current_depth + 1);
    }
}

enum Entry<'a> {
    Dir(String, &'a DirNode),
    File(String, Count),
}

fn main() {
    let cli = Cli::parse();

    let paths: Vec<&str> = cli.paths.iter().map(|s| s.as_str()).collect();
    let excluded: Vec<&str> = cli.exclude.iter().map(|s| s.as_str()).collect();

    let mut config = Config::default();
    if cli.hidden {
        config.hidden = Some(true);
    }
    if cli.no_ignore {
        config.no_ignore = Some(true);
    }
    if cli.no_ignore_parent {
        config.no_ignore_parent = Some(true);
    }
    if cli.no_ignore_dot {
        config.no_ignore_dot = Some(true);
    }
    if cli.no_ignore_vcs {
        config.no_ignore_vcs = Some(true);
    }
    if !cli.types.is_empty() {
        let types: Vec<LanguageType> = cli
            .types
            .iter()
            .flat_map(|s| s.split(','))
            .filter_map(|s| s.parse::<LanguageType>().ok())
            .collect();
        config.types = Some(types);
    }

    let mut languages = Languages::new();
    languages.get_statistics(&paths, &excluded, &config);

    // Collect per-file stats
    let mut file_stats: BTreeMap<PathBuf, Count> = BTreeMap::new();
    for (lang_type, language) in &languages {
        let is_literate = lang_type.is_literate();
        for report in &language.reports {
            let path = report.name.clone();
            let stats = &report.stats;
            let count = if is_literate {
                Count {
                    code: 0,
                    comments: 0,
                    literate: stats.code + stats.comments + stats.blanks,
                }
            } else {
                Count {
                    code: stats.code,
                    comments: stats.comments,
                    literate: 0,
                }
            };
            let entry = file_stats.entry(path).or_insert(Count::new());
            *entry += count;
        }
    }

    // Determine the base path for stripping prefixes
    let (base, base_as_string) = if cli.paths.len() == 1 {
        let p = Path::new(&cli.paths[0]);
        let base = if p.is_dir() {
            std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
        } else {
            p.to_path_buf()
        };

        (base, cli.paths[0].clone())
    } else {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        (base, ".".to_string())
    };

    // Build tree
    let mut root = DirNode::new(base_as_string.clone());
    for (path, count) in &file_stats {
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        let relative = canonical.strip_prefix(&base).unwrap_or(&canonical);
        let components: Vec<&str> = relative
            .components()
            .map(|c| c.as_os_str().to_str().unwrap_or("?"))
            .collect();
        if !components.is_empty() {
            root.insert_file(&components, *count);
        }
    }
    root.compute_totals();

    // Render
    println!("{} {}", base_as_string.blue(), root.total_count);
    if cli.dir_mode {
        render_dir_mode(&root, "", cli.direct, cli.depth, 0);
    } else {
        render_file_mode(&root, "");
    }
}

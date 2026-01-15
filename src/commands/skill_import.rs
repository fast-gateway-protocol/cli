//! Import skills from agent-specific formats to canonical FGP format.
//!
//! This module provides reverse-engineering capabilities to convert existing
//! skills from various AI agent ecosystems (Claude Code, Cursor, Codex, etc.)
//! into the canonical FGP `skill.yaml` format.
//!
//! # Supported Import Formats
//!
//! - **Claude Code** (SKILL.md): ~80% fidelity - YAML frontmatter + markdown
//! - **Cursor** (.cursorrules): ~50% fidelity - pure markdown
//! - **Codex** (.codex.json): ~25% fidelity - minimal JSON schema
//! - **MCP** (.mcp.json): ~30% fidelity - tool schema
//! - **Gemini** (gemini-extension.json): ~75% fidelity - JSON manifest
//!
//! # Daemon Registry Enrichment
//!
//! The import system can enrich imported skills with metadata from FGP daemon
//! manifest files, providing:
//! - Full method descriptions and parameter schemas
//! - Authentication requirements
//! - Platform compatibility information
//!
//! # Usage
//!
//! ```bash
//! fgp skill import ./SKILL.md --output ./imported-skill/
//! fgp skill import ./rules.txt --format cursor
//! fgp skill import ./SKILL.md --dry-run
//! fgp skill import ./SKILL.md --enrich  # Enable daemon registry enrichment
//! ```

use anyhow::{bail, Context, Result};
use colored::Colorize;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

// ============================================================================
// Unified Intermediate Representation (UIR)
// ============================================================================

/// Confidence level for imported fields
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    /// Directly extracted from structured data
    High,
    /// Inferred from patterns/context
    Medium,
    /// Guessed or placeholder
    Low,
    /// Could not determine, needs user input
    Unknown,
}

impl Confidence {
    fn symbol(&self) -> &'static str {
        match self {
            Confidence::High => "✓",
            Confidence::Medium => "⚠",
            Confidence::Low => "?",
            Confidence::Unknown => "✗",
        }
    }

    fn color_str(&self, s: &str) -> String {
        match self {
            Confidence::High => s.green().to_string(),
            Confidence::Medium => s.yellow().to_string(),
            Confidence::Low => s.red().to_string(),
            Confidence::Unknown => s.dimmed().to_string(),
        }
    }
}

/// Source of an imported field
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSource {
    /// From YAML/JSON frontmatter
    Frontmatter,
    /// From markdown headers/content
    Content,
    /// From filename/path
    Filename,
    /// Inferred from method calls in text
    MethodExtraction,
    /// Looked up from daemon registry
    Registry,
    /// User-provided during import
    UserInput,
    /// Default/placeholder value
    Default,
}

/// An imported field with confidence metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedField<T> {
    pub value: T,
    pub confidence: Confidence,
    pub source: FieldSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl<T> ImportedField<T> {
    pub fn high(value: T, source: FieldSource) -> Self {
        Self {
            value,
            confidence: Confidence::High,
            source,
            notes: None,
        }
    }

    pub fn medium(value: T, source: FieldSource) -> Self {
        Self {
            value,
            confidence: Confidence::Medium,
            source,
            notes: None,
        }
    }

    pub fn low(value: T, source: FieldSource) -> Self {
        Self {
            value,
            confidence: Confidence::Low,
            source,
            notes: None,
        }
    }

    pub fn unknown(value: T) -> Self {
        Self {
            value,
            confidence: Confidence::Unknown,
            source: FieldSource::Default,
            notes: Some("Could not determine value".to_string()),
        }
    }

    pub fn with_note(mut self, note: &str) -> Self {
        self.notes = Some(note.to_string());
        self
    }
}

/// Imported daemon dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedDaemon {
    pub name: ImportedField<String>,
    pub version: ImportedField<Option<String>>,
    pub optional: ImportedField<bool>,
    pub methods: Vec<ImportedField<String>>,
}

/// Imported trigger configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportedTriggers {
    pub keywords: Vec<ImportedField<String>>,
    pub patterns: Vec<ImportedField<String>>,
    pub commands: Vec<ImportedField<String>>,
}

/// Imported author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedAuthor {
    pub name: ImportedField<String>,
    pub email: ImportedField<Option<String>>,
    pub url: ImportedField<Option<String>>,
}

/// The source format being imported from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImportFormat {
    ClaudeCode,
    Cursor,
    Codex,
    Mcp,
    Zed,
    Windsurf,
    Gemini,
    Aider,
}

impl ImportFormat {
    pub fn detect(path: &Path) -> Option<Self> {
        let filename = path.file_name()?.to_str()?;
        let extension = path.extension().and_then(|e| e.to_str());

        // Check by filename patterns
        if filename == "SKILL.md" {
            return Some(ImportFormat::ClaudeCode);
        }
        if filename.ends_with(".cursorrules") || filename == ".cursorrules" {
            return Some(ImportFormat::Cursor);
        }
        if filename.ends_with(".codex.json") {
            return Some(ImportFormat::Codex);
        }
        if filename.ends_with(".mcp.json") {
            return Some(ImportFormat::Mcp);
        }
        if filename.ends_with(".rules") {
            return Some(ImportFormat::Zed);
        }
        if filename.ends_with(".windsurf.md") {
            return Some(ImportFormat::Windsurf);
        }
        if filename == "gemini-extension.json" {
            return Some(ImportFormat::Gemini);
        }
        if filename.ends_with(".CONVENTIONS.md") || filename == "CONVENTIONS.md" {
            return Some(ImportFormat::Aider);
        }

        // Check by extension
        match extension {
            Some("md") => {
                // Could be Claude Code or other markdown
                // Read first few lines to detect
                None
            }
            Some("json") => None,
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            ImportFormat::ClaudeCode => "Claude Code",
            ImportFormat::Cursor => "Cursor",
            ImportFormat::Codex => "Codex",
            ImportFormat::Mcp => "MCP",
            ImportFormat::Zed => "Zed",
            ImportFormat::Windsurf => "Windsurf",
            ImportFormat::Gemini => "Gemini",
            ImportFormat::Aider => "Aider",
        }
    }

    pub fn to_key(&self) -> &'static str {
        match self {
            ImportFormat::ClaudeCode => "claude-code",
            ImportFormat::Cursor => "cursor",
            ImportFormat::Codex => "codex",
            ImportFormat::Mcp => "mcp",
            ImportFormat::Zed => "zed",
            ImportFormat::Windsurf => "windsurf",
            ImportFormat::Gemini => "gemini",
            ImportFormat::Aider => "aider",
        }
    }
}

/// Unified Intermediate Representation for imported skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportedSkill {
    // === METADATA ===
    pub name: ImportedField<String>,
    pub version: ImportedField<String>,
    pub description: ImportedField<String>,
    pub author: Option<ImportedAuthor>,

    // === DAEMONS ===
    pub daemons: Vec<ImportedDaemon>,

    // === INSTRUCTIONS ===
    pub instructions_content: ImportedField<String>,

    // === TRIGGERS ===
    pub triggers: ImportedTriggers,

    // === SOURCE METADATA ===
    pub source_format: ImportFormat,
    pub source_path: PathBuf,
    pub import_timestamp: String,
}

impl ImportedSkill {
    /// Calculate overall confidence score (0-100)
    pub fn confidence_score(&self) -> u32 {
        let mut total = 0u32;
        let mut count = 0u32;

        let conf_value = |c: Confidence| match c {
            Confidence::High => 100,
            Confidence::Medium => 60,
            Confidence::Low => 30,
            Confidence::Unknown => 0,
        };

        // Core fields
        total += conf_value(self.name.confidence);
        count += 1;
        total += conf_value(self.version.confidence);
        count += 1;
        total += conf_value(self.description.confidence);
        count += 1;
        total += conf_value(self.instructions_content.confidence);
        count += 1;

        // Daemons
        for daemon in &self.daemons {
            total += conf_value(daemon.name.confidence);
            count += 1;
            for method in &daemon.methods {
                total += conf_value(method.confidence);
                count += 1;
            }
        }

        // Triggers
        for kw in &self.triggers.keywords {
            total += conf_value(kw.confidence);
            count += 1;
        }

        if count == 0 {
            return 0;
        }

        total / count
    }
}

// ============================================================================
// Daemon Registry for Enrichment
// ============================================================================

/// Daemon manifest structure (from {daemon}/manifest.json)
#[derive(Debug, Clone, Deserialize)]
pub struct DaemonManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub methods: Vec<ManifestMethod>,
    #[serde(default)]
    pub auth: Option<ManifestAuth>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

/// Method definition from manifest
#[derive(Debug, Clone, Deserialize)]
pub struct ManifestMethod {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub params: Vec<ManifestParam>,
}

/// Parameter definition from manifest
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestParam {
    pub name: String,
    #[serde(rename = "type", default)]
    pub param_type: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Authentication configuration from manifest
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestAuth {
    #[serde(rename = "type")]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Registry of known FGP daemons for enrichment
#[derive(Debug, Default)]
pub struct DaemonRegistry {
    /// Map of daemon name -> manifest
    daemons: HashMap<String, DaemonManifest>,
    /// Map of method name -> (daemon name, method info)
    methods: HashMap<String, (String, ManifestMethod)>,
}

impl DaemonRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Load daemons from the FGP project directory
    pub fn load_from_fgp_dir(fgp_dir: &Path) -> Result<Self> {
        let mut registry = Self::new();

        // Known daemon directories
        let daemon_dirs = [
            "gmail", "calendar", "github", "browser", "imessage",
            "vercel", "fly", "neon", "travel", "slack",
        ];

        for daemon_name in daemon_dirs {
            let manifest_path = fgp_dir.join(daemon_name).join("manifest.json");
            if manifest_path.exists() {
                match fs::read_to_string(&manifest_path) {
                    Ok(content) => {
                        match serde_json::from_str::<DaemonManifest>(&content) {
                            Ok(manifest) => {
                                registry.add_daemon(manifest);
                            }
                            Err(e) => {
                                eprintln!(
                                    "Warning: Failed to parse {}: {}",
                                    manifest_path.display(),
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Failed to read {}: {}",
                            manifest_path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(registry)
    }

    /// Load from default FGP directory (~/.fgp or ~/Projects/fgp)
    pub fn load_default() -> Result<Self> {
        // Try common FGP project locations
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

        let possible_paths = [
            home.join("Projects").join("fgp"),
            home.join("projects").join("fgp"),
            home.join(".fgp").join("src"),
        ];

        for path in &possible_paths {
            if path.exists() {
                return Self::load_from_fgp_dir(path);
            }
        }

        // Return empty registry if no FGP directory found
        Ok(Self::new())
    }

    /// Add a daemon manifest to the registry
    fn add_daemon(&mut self, manifest: DaemonManifest) {
        // Index all methods
        for method in &manifest.methods {
            self.methods.insert(
                method.name.clone(),
                (manifest.name.clone(), method.clone()),
            );

            // Also index without namespace prefix
            if let Some(short_name) = method.name.split('.').last() {
                let key = format!("{}.{}", manifest.name, short_name);
                if !self.methods.contains_key(&key) {
                    self.methods.insert(key, (manifest.name.clone(), method.clone()));
                }
            }
        }

        self.daemons.insert(manifest.name.clone(), manifest);
    }

    /// Get daemon manifest by name
    pub fn get_daemon(&self, name: &str) -> Option<&DaemonManifest> {
        self.daemons.get(name)
    }

    /// Get method info by full method name (e.g., "gmail.send")
    pub fn get_method(&self, method_name: &str) -> Option<&(String, ManifestMethod)> {
        self.methods.get(method_name)
    }

    /// Get all methods for a daemon
    pub fn get_daemon_methods(&self, daemon_name: &str) -> Vec<&ManifestMethod> {
        self.daemons
            .get(daemon_name)
            .map(|d| d.methods.iter().collect())
            .unwrap_or_default()
    }

    /// Check if a daemon is known
    pub fn has_daemon(&self, name: &str) -> bool {
        self.daemons.contains_key(name)
    }

    /// Get number of loaded daemons
    pub fn daemon_count(&self) -> usize {
        self.daemons.len()
    }

    /// Get all daemon names
    pub fn daemon_names(&self) -> Vec<&str> {
        self.daemons.keys().map(|s| s.as_str()).collect()
    }
}

/// Enrichment data added from registry
#[derive(Debug, Clone, Default, Serialize)]
pub struct EnrichmentData {
    /// Methods with descriptions from registry
    pub method_descriptions: HashMap<String, String>,
    /// Method parameters from registry
    pub method_params: HashMap<String, Vec<ManifestParam>>,
    /// Authentication requirements
    pub auth_requirements: HashMap<String, ManifestAuth>,
    /// Platform support
    pub platform_support: HashMap<String, Vec<String>>,
    /// Daemons found in registry
    pub verified_daemons: Vec<String>,
    /// Daemons not found in registry
    pub unknown_daemons: Vec<String>,
}

/// Enrich an imported skill with data from the daemon registry
pub fn enrich_skill(skill: &mut ImportedSkill, registry: &DaemonRegistry) -> EnrichmentData {
    let mut enrichment = EnrichmentData::default();

    for daemon in &mut skill.daemons {
        let daemon_name = &daemon.name.value;

        if let Some(manifest) = registry.get_daemon(daemon_name) {
            enrichment.verified_daemons.push(daemon_name.clone());

            // Upgrade confidence if verified
            if daemon.name.confidence == Confidence::Low {
                daemon.name.confidence = Confidence::Medium;
                daemon.name.notes = Some("Verified against daemon registry".to_string());
            } else if daemon.name.confidence == Confidence::Medium {
                daemon.name.confidence = Confidence::High;
                daemon.name.notes = Some("Confirmed in daemon registry".to_string());
            }

            // Add auth requirements
            if let Some(ref auth) = manifest.auth {
                enrichment
                    .auth_requirements
                    .insert(daemon_name.clone(), auth.clone());
            }

            // Add platform support
            if !manifest.platforms.is_empty() {
                enrichment
                    .platform_support
                    .insert(daemon_name.clone(), manifest.platforms.clone());
            }

            // Enrich methods
            for method in &mut daemon.methods {
                let full_method_name = format!("{}.{}", daemon_name, method.value);

                if let Some((_, manifest_method)) = registry.get_method(&full_method_name) {
                    // Add description
                    if let Some(ref desc) = manifest_method.description {
                        enrichment
                            .method_descriptions
                            .insert(full_method_name.clone(), desc.clone());
                    }

                    // Add parameters
                    if !manifest_method.params.is_empty() {
                        enrichment
                            .method_params
                            .insert(full_method_name.clone(), manifest_method.params.clone());
                    }

                    // Upgrade method confidence
                    if method.confidence != Confidence::High {
                        method.confidence = Confidence::High;
                        method.notes = Some("Verified in daemon registry".to_string());
                    }
                }
            }

            // Add any missing methods from registry
            let known_methods: Vec<String> = daemon.methods.iter().map(|m| m.value.clone()).collect();
            for manifest_method in &manifest.methods {
                let short_name = manifest_method
                    .name
                    .split('.')
                    .last()
                    .unwrap_or(&manifest_method.name);

                if !known_methods.contains(&short_name.to_string()) {
                    // Don't add missing methods automatically, but note them
                    let full_name = format!("{}.{}", daemon_name, short_name);
                    if let Some(ref desc) = manifest_method.description {
                        enrichment.method_descriptions.insert(full_name, desc.clone());
                    }
                }
            }
        } else {
            enrichment.unknown_daemons.push(daemon_name.clone());
        }
    }

    enrichment
}

// ============================================================================
// Quality Scoring & Import Recommendations
// ============================================================================

/// Quality grade (A-F) based on import completeness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum QualityGrade {
    A, // 90-100%: Production-ready
    B, // 80-89%: Good, minor issues
    C, // 70-79%: Usable, needs review
    D, // 60-69%: Incomplete, needs work
    F, // Below 60%: Significant issues
}

impl QualityGrade {
    pub fn from_score(score: u32) -> Self {
        match score {
            90..=100 => QualityGrade::A,
            80..=89 => QualityGrade::B,
            70..=79 => QualityGrade::C,
            60..=69 => QualityGrade::D,
            _ => QualityGrade::F,
        }
    }

    pub fn emoji(&self) -> &'static str {
        match self {
            QualityGrade::A => "🟢",
            QualityGrade::B => "🔵",
            QualityGrade::C => "🟡",
            QualityGrade::D => "🟠",
            QualityGrade::F => "🔴",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            QualityGrade::A => "Production Ready",
            QualityGrade::B => "Good - Minor Issues",
            QualityGrade::C => "Usable - Needs Review",
            QualityGrade::D => "Incomplete - Needs Work",
            QualityGrade::F => "Significant Issues",
        }
    }
}

/// Priority level for recommendations
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Priority {
    /// Must fix before using
    Critical,
    /// Should fix soon
    High,
    /// Recommended improvement
    Medium,
    /// Nice to have
    Low,
}

impl Priority {
    pub fn emoji(&self) -> &'static str {
        match self {
            Priority::Critical => "🚨",
            Priority::High => "⚠️",
            Priority::Medium => "📝",
            Priority::Low => "💡",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Priority::Critical => "CRITICAL",
            Priority::High => "HIGH",
            Priority::Medium => "MEDIUM",
            Priority::Low => "LOW",
        }
    }
}

/// Category of quality issue
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IssueCategory {
    /// Missing required field
    MissingRequired,
    /// Low confidence field
    LowConfidence,
    /// Unverified daemon
    UnverifiedDaemon,
    /// Missing authentication info
    MissingAuth,
    /// No triggers defined
    NoTriggers,
    /// Placeholder/default value
    PlaceholderValue,
    /// Format limitation
    FormatLimitation,
}

/// A specific import quality issue
#[derive(Debug, Clone, Serialize)]
pub struct QualityIssue {
    pub category: IssueCategory,
    pub priority: Priority,
    pub field: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// An actionable recommendation
#[derive(Debug, Clone, Serialize)]
pub struct ImportRecommendation {
    pub priority: Priority,
    pub title: String,
    pub description: String,
    pub action: String,
    /// Estimated effort: "quick" (<5 min), "moderate" (5-15 min), "significant" (>15 min)
    pub effort: &'static str,
}

/// Comprehensive quality assessment of an imported skill
#[derive(Debug, Clone, Serialize)]
pub struct QualityAssessment {
    /// Overall grade
    pub grade: QualityGrade,
    /// Numeric score (0-100)
    pub score: u32,
    /// Breakdown by category
    pub breakdown: QualityBreakdown,
    /// All detected issues
    pub issues: Vec<QualityIssue>,
    /// Prioritized recommendations
    pub recommendations: Vec<ImportRecommendation>,
    /// Format-specific limitations
    pub format_limitations: Vec<String>,
}

/// Detailed score breakdown
#[derive(Debug, Clone, Serialize)]
pub struct QualityBreakdown {
    /// Metadata completeness (name, version, description, author, license)
    pub metadata_score: u32,
    /// Daemon configuration (names, methods, versions)
    pub daemon_score: u32,
    /// Instructions quality
    pub instructions_score: u32,
    /// Trigger configuration
    pub trigger_score: u32,
    /// Auth/config completeness (bonus points when enriched)
    pub config_score: u32,
}

impl QualityBreakdown {
    /// Calculate weighted overall score
    pub fn overall(&self) -> u32 {
        // Weights: metadata=25%, daemons=30%, instructions=25%, triggers=10%, config=10%
        let weighted = self.metadata_score * 25
            + self.daemon_score * 30
            + self.instructions_score * 25
            + self.trigger_score * 10
            + self.config_score * 10;
        weighted / 100
    }
}

/// Analyze quality of an imported skill
pub fn analyze_quality(
    skill: &ImportedSkill,
    enrichment: Option<&EnrichmentData>,
) -> QualityAssessment {
    let mut issues = Vec::new();
    let mut recommendations = Vec::new();

    // === METADATA SCORING ===
    let mut metadata_score = 0u32;

    // Name (0-20)
    match skill.name.confidence {
        Confidence::High => metadata_score += 20,
        Confidence::Medium => {
            metadata_score += 15;
            issues.push(QualityIssue {
                category: IssueCategory::LowConfidence,
                priority: Priority::Low,
                field: "name".to_string(),
                message: "Skill name has medium confidence".to_string(),
                suggestion: Some("Verify the name is correct".to_string()),
            });
        }
        Confidence::Low | Confidence::Unknown => {
            metadata_score += 5;
            issues.push(QualityIssue {
                category: IssueCategory::LowConfidence,
                priority: Priority::High,
                field: "name".to_string(),
                message: "Skill name has low confidence".to_string(),
                suggestion: Some("Update name in skill.yaml".to_string()),
            });
        }
    }

    // Version (0-15)
    match skill.version.confidence {
        Confidence::High => metadata_score += 15,
        Confidence::Medium => metadata_score += 10,
        Confidence::Low => {
            metadata_score += 5;
            issues.push(QualityIssue {
                category: IssueCategory::PlaceholderValue,
                priority: Priority::Medium,
                field: "version".to_string(),
                message: "Version is a placeholder value".to_string(),
                suggestion: Some("Set actual version (e.g., 1.0.0)".to_string()),
            });
        }
        Confidence::Unknown => {
            issues.push(QualityIssue {
                category: IssueCategory::MissingRequired,
                priority: Priority::High,
                field: "version".to_string(),
                message: "Version could not be determined".to_string(),
                suggestion: Some("Add version field to skill.yaml".to_string()),
            });
        }
    }

    // Description (0-25)
    match skill.description.confidence {
        Confidence::High => {
            metadata_score += 25;
            // Check description quality
            if skill.description.value.len() < 20 {
                metadata_score -= 5;
                issues.push(QualityIssue {
                    category: IssueCategory::LowConfidence,
                    priority: Priority::Low,
                    field: "description".to_string(),
                    message: "Description is very short".to_string(),
                    suggestion: Some("Add more detail about what the skill does".to_string()),
                });
            }
        }
        Confidence::Medium => {
            metadata_score += 15;
        }
        Confidence::Low | Confidence::Unknown => {
            metadata_score += 5;
            issues.push(QualityIssue {
                category: IssueCategory::LowConfidence,
                priority: Priority::Medium,
                field: "description".to_string(),
                message: "Description has low confidence".to_string(),
                suggestion: Some("Write a clear description of the skill's purpose".to_string()),
            });
        }
    }

    // Author (0-20)
    if let Some(ref author) = skill.author {
        if author.name.confidence == Confidence::High {
            metadata_score += 20;
        } else if author.name.value != "Unknown" {
            metadata_score += 10;
        } else {
            metadata_score += 2;
            issues.push(QualityIssue {
                category: IssueCategory::MissingRequired,
                priority: Priority::Medium,
                field: "author".to_string(),
                message: "Author information is missing".to_string(),
                suggestion: Some("Add author name and email".to_string()),
            });
        }
    } else {
        issues.push(QualityIssue {
            category: IssueCategory::MissingRequired,
            priority: Priority::Medium,
            field: "author".to_string(),
            message: "No author information".to_string(),
            suggestion: Some("Add author section to skill.yaml".to_string()),
        });
    }

    // License (0-20)
    // License is typically low confidence in imports
    metadata_score += 10; // Default credit for having any license

    // === DAEMON SCORING ===
    let mut daemon_score = 0u32;

    if skill.daemons.is_empty() {
        issues.push(QualityIssue {
            category: IssueCategory::MissingRequired,
            priority: Priority::Critical,
            field: "daemons".to_string(),
            message: "No daemons detected".to_string(),
            suggestion: Some("Add daemon dependencies to skill.yaml".to_string()),
        });
    } else {
        let daemon_count = skill.daemons.len();
        let method_count: usize = skill.daemons.iter().map(|d| d.methods.len()).sum();

        // Base score for having daemons
        daemon_score += 30.min(daemon_count as u32 * 15);

        // Method coverage
        if method_count > 0 {
            daemon_score += 30.min(method_count as u32 * 5);
        }

        // Check enrichment verification
        if let Some(e) = enrichment {
            // Bonus for verified daemons
            let verified_ratio = if daemon_count > 0 {
                (e.verified_daemons.len() * 100 / daemon_count) as u32
            } else {
                0
            };
            daemon_score += (40 * verified_ratio / 100).min(40);

            // Issues for unverified daemons
            for unknown in &e.unknown_daemons {
                issues.push(QualityIssue {
                    category: IssueCategory::UnverifiedDaemon,
                    priority: Priority::High,
                    field: format!("daemon.{}", unknown),
                    message: format!("Daemon '{}' not found in registry", unknown),
                    suggestion: Some("Verify daemon name or check if it's a custom daemon".to_string()),
                });
            }
        } else {
            // Partial credit without enrichment
            daemon_score += 20;
            issues.push(QualityIssue {
                category: IssueCategory::UnverifiedDaemon,
                priority: Priority::Medium,
                field: "daemons".to_string(),
                message: "Daemons not verified against registry".to_string(),
                suggestion: Some("Run with --enrich flag to verify daemons".to_string()),
            });
        }
    }

    // === INSTRUCTIONS SCORING ===
    let mut instructions_score = 0u32;

    match skill.instructions_content.confidence {
        Confidence::High => instructions_score += 50,
        Confidence::Medium => instructions_score += 35,
        Confidence::Low => instructions_score += 20,
        Confidence::Unknown => instructions_score += 5,
    }

    // Check instruction content quality
    let instruction_len = skill.instructions_content.value.len();
    if instruction_len > 500 {
        instructions_score += 30;
    } else if instruction_len > 200 {
        instructions_score += 20;
    } else if instruction_len > 50 {
        instructions_score += 10;
    } else {
        issues.push(QualityIssue {
            category: IssueCategory::LowConfidence,
            priority: Priority::High,
            field: "instructions".to_string(),
            message: "Instructions are very brief".to_string(),
            suggestion: Some("Add detailed usage instructions".to_string()),
        });
    }

    // Check for code examples (bonus)
    if skill.instructions_content.value.contains("```") {
        instructions_score += 20;
    }

    // Cap at 100
    instructions_score = instructions_score.min(100);

    // === TRIGGER SCORING ===
    let mut trigger_score = 0u32;
    let trigger_count = skill.triggers.keywords.len()
        + skill.triggers.patterns.len()
        + skill.triggers.commands.len();

    if trigger_count == 0 {
        issues.push(QualityIssue {
            category: IssueCategory::NoTriggers,
            priority: Priority::Medium,
            field: "triggers".to_string(),
            message: "No triggers defined".to_string(),
            suggestion: Some("Add keywords and patterns to help agents invoke the skill".to_string()),
        });
    } else {
        // Keywords
        trigger_score += (skill.triggers.keywords.len() as u32 * 15).min(45);
        // Patterns
        trigger_score += (skill.triggers.patterns.len() as u32 * 20).min(40);
        // Commands
        trigger_score += (skill.triggers.commands.len() as u32 * 10).min(15);
    }

    trigger_score = trigger_score.min(100);

    // === CONFIG SCORING ===
    let mut config_score = 0u32;

    // Auth requirements from enrichment
    if let Some(e) = enrichment {
        if !e.auth_requirements.is_empty() {
            config_score += 40;
        }
        if !e.platform_support.is_empty() {
            config_score += 20;
        }
        // Method documentation bonus
        if !e.method_descriptions.is_empty() {
            let ratio = (e.method_descriptions.len() * 100)
                / skill.daemons.iter().map(|d| d.methods.len()).sum::<usize>().max(1);
            config_score += (ratio as u32 * 40 / 100).min(40);
        }
    } else {
        // Without enrichment, config is mostly unknown
        config_score += 20;
        issues.push(QualityIssue {
            category: IssueCategory::MissingAuth,
            priority: Priority::Medium,
            field: "auth".to_string(),
            message: "Authentication requirements unknown".to_string(),
            suggestion: Some("Run with --enrich or manually check daemon auth needs".to_string()),
        });
    }

    config_score = config_score.min(100);

    // === BUILD RECOMMENDATIONS ===

    // Sort issues by priority
    issues.sort_by(|a, b| a.priority.cmp(&b.priority));

    // Generate recommendations from issues
    let critical_count = issues.iter().filter(|i| i.priority == Priority::Critical).count();
    let high_count = issues.iter().filter(|i| i.priority == Priority::High).count();

    if critical_count > 0 {
        recommendations.push(ImportRecommendation {
            priority: Priority::Critical,
            title: "Fix Critical Issues First".to_string(),
            description: format!(
                "{} critical issue(s) must be resolved before the skill can be used",
                critical_count
            ),
            action: "Review and fix all 🚨 CRITICAL issues in the list above".to_string(),
            effort: "moderate",
        });
    }

    if high_count > 0 {
        recommendations.push(ImportRecommendation {
            priority: Priority::High,
            title: "Address High Priority Issues".to_string(),
            description: format!(
                "{} high priority issue(s) should be fixed for reliable operation",
                high_count
            ),
            action: "Review and fix all ⚠️ HIGH issues".to_string(),
            effort: "moderate",
        });
    }

    // Common recommendations based on score
    let breakdown = QualityBreakdown {
        metadata_score,
        daemon_score,
        instructions_score,
        trigger_score,
        config_score,
    };

    if breakdown.metadata_score < 70 {
        recommendations.push(ImportRecommendation {
            priority: Priority::Medium,
            title: "Complete Metadata".to_string(),
            description: "Skill metadata is incomplete".to_string(),
            action: "Add author info, verify version and license in skill.yaml".to_string(),
            effort: "quick",
        });
    }

    if breakdown.daemon_score < 70 && enrichment.is_none() {
        recommendations.push(ImportRecommendation {
            priority: Priority::High,
            title: "Verify Daemon Configuration".to_string(),
            description: "Daemon dependencies haven't been verified".to_string(),
            action: "Re-run import with --enrich flag to validate daemons".to_string(),
            effort: "quick",
        });
    }

    if breakdown.instructions_score < 70 {
        recommendations.push(ImportRecommendation {
            priority: Priority::Medium,
            title: "Improve Instructions".to_string(),
            description: "Instructions could be more comprehensive".to_string(),
            action: "Expand instructions/core.md with examples and usage patterns".to_string(),
            effort: "moderate",
        });
    }

    if breakdown.trigger_score < 50 {
        recommendations.push(ImportRecommendation {
            priority: Priority::Low,
            title: "Add Triggers".to_string(),
            description: "No triggers configured for agent discovery".to_string(),
            action: "Add keywords and patterns to triggers section in skill.yaml".to_string(),
            effort: "quick",
        });
    }

    // Format-specific limitations
    let format_limitations = get_format_limitations(skill.source_format);

    // Calculate final score
    let score = breakdown.overall();
    let grade = QualityGrade::from_score(score);

    // Add grade-specific recommendation
    match grade {
        QualityGrade::A => {
            recommendations.push(ImportRecommendation {
                priority: Priority::Low,
                title: "Ready for Use".to_string(),
                description: "This skill has excellent import quality".to_string(),
                action: "Review any remaining issues and test the skill".to_string(),
                effort: "quick",
            });
        }
        QualityGrade::B => {
            recommendations.push(ImportRecommendation {
                priority: Priority::Low,
                title: "Good Quality Import".to_string(),
                description: "Most data recovered successfully".to_string(),
                action: "Address medium priority items when convenient".to_string(),
                effort: "quick",
            });
        }
        QualityGrade::C | QualityGrade::D => {
            recommendations.push(ImportRecommendation {
                priority: Priority::Medium,
                title: "Manual Review Required".to_string(),
                description: "Import quality is below ideal".to_string(),
                action: "Review skill.yaml carefully and fill in missing data".to_string(),
                effort: "moderate",
            });
        }
        QualityGrade::F => {
            recommendations.push(ImportRecommendation {
                priority: Priority::High,
                title: "Significant Work Needed".to_string(),
                description: "Import recovered limited data".to_string(),
                action: "Consider using this as a template and rewriting most fields".to_string(),
                effort: "significant",
            });
        }
    }

    // Sort recommendations by priority
    recommendations.sort_by(|a, b| a.priority.cmp(&b.priority));

    QualityAssessment {
        grade,
        score,
        breakdown,
        issues,
        recommendations,
        format_limitations,
    }
}

/// Get known limitations for each import format
fn get_format_limitations(format: ImportFormat) -> Vec<String> {
    match format {
        ImportFormat::ClaudeCode => vec![
            "Workflows not included in export format".to_string(),
            "Config options not recoverable".to_string(),
            "Some triggers may be inferred".to_string(),
        ],
        ImportFormat::Cursor => vec![
            "No structured metadata in format".to_string(),
            "Daemon/method info must be inferred from text".to_string(),
            "No version or author information".to_string(),
            "Pure markdown format has low fidelity (~50%)".to_string(),
        ],
        ImportFormat::Codex => vec![
            "Minimal schema format (~25% fidelity)".to_string(),
            "No detailed instructions".to_string(),
            "Tool list only, no method parameters".to_string(),
        ],
        ImportFormat::Mcp => vec![
            "Tool definitions only (~30% fidelity)".to_string(),
            "No workflow or trigger information".to_string(),
            "Method names may need translation".to_string(),
        ],
        ImportFormat::Zed => vec![
            "Context-only format (~40% fidelity)".to_string(),
            "No structured daemon configuration".to_string(),
            "Rules may not map to FGP concepts".to_string(),
        ],
        ImportFormat::Windsurf => vec![
            "Similar limitations to Claude Code".to_string(),
            "Instructions may need adaptation".to_string(),
        ],
        ImportFormat::Gemini => vec![
            "Capabilities may not map directly to FGP methods".to_string(),
            "Extension config not all recoverable".to_string(),
        ],
        ImportFormat::Aider => vec![
            "Conventions format is minimal (~35% fidelity)".to_string(),
            "No tool/daemon definitions".to_string(),
            "Style preferences only".to_string(),
        ],
    }
}

// ============================================================================
// Two-Way Sync Detection
// ============================================================================

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Fingerprint of a skill for change detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFingerprint {
    /// Hash of name
    pub name_hash: u64,
    /// Hash of description
    pub description_hash: u64,
    /// Hash of version
    pub version_hash: u64,
    /// Hash of all daemon names and methods
    pub daemons_hash: u64,
    /// Hash of instructions content
    pub instructions_hash: u64,
    /// Hash of triggers
    pub triggers_hash: u64,
    /// Combined hash of everything
    pub combined_hash: u64,
    /// Timestamp when fingerprint was created
    pub timestamp: String,
}

impl SkillFingerprint {
    /// Create fingerprint from imported skill
    pub fn from_imported(skill: &ImportedSkill) -> Self {
        let mut name_hasher = DefaultHasher::new();
        skill.name.value.hash(&mut name_hasher);
        let name_hash = name_hasher.finish();

        let mut desc_hasher = DefaultHasher::new();
        skill.description.value.hash(&mut desc_hasher);
        let description_hash = desc_hasher.finish();

        let mut version_hasher = DefaultHasher::new();
        skill.version.value.hash(&mut version_hasher);
        let version_hash = version_hasher.finish();

        let mut daemons_hasher = DefaultHasher::new();
        for daemon in &skill.daemons {
            daemon.name.value.hash(&mut daemons_hasher);
            for method in &daemon.methods {
                method.value.hash(&mut daemons_hasher);
            }
        }
        let daemons_hash = daemons_hasher.finish();

        let mut instructions_hasher = DefaultHasher::new();
        skill.instructions_content.value.hash(&mut instructions_hasher);
        let instructions_hash = instructions_hasher.finish();

        let mut triggers_hasher = DefaultHasher::new();
        for kw in &skill.triggers.keywords {
            kw.value.hash(&mut triggers_hasher);
        }
        for pattern in &skill.triggers.patterns {
            pattern.value.hash(&mut triggers_hasher);
        }
        let triggers_hash = triggers_hasher.finish();

        // Combined hash
        let mut combined_hasher = DefaultHasher::new();
        name_hash.hash(&mut combined_hasher);
        description_hash.hash(&mut combined_hasher);
        version_hash.hash(&mut combined_hasher);
        daemons_hash.hash(&mut combined_hasher);
        instructions_hash.hash(&mut combined_hasher);
        triggers_hash.hash(&mut combined_hasher);
        let combined_hash = combined_hasher.finish();

        Self {
            name_hash,
            description_hash,
            version_hash,
            daemons_hash,
            instructions_hash,
            triggers_hash,
            combined_hash,
            timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }
}

/// Type of change detected between two skill versions
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ChangeType {
    /// No change
    Unchanged,
    /// Content was added
    Added,
    /// Content was removed
    Removed,
    /// Content was modified
    Modified,
}

impl ChangeType {
    pub fn emoji(&self) -> &'static str {
        match self {
            ChangeType::Unchanged => "✓",
            ChangeType::Added => "+",
            ChangeType::Removed => "-",
            ChangeType::Modified => "~",
        }
    }
}

/// A specific field-level diff
#[derive(Debug, Clone, Serialize)]
pub struct FieldDiff {
    pub field: String,
    pub change_type: ChangeType,
    pub original_value: Option<String>,
    pub current_value: Option<String>,
    pub significance: DiffSignificance,
}

/// How significant is this diff
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DiffSignificance {
    /// Critical: affects functionality
    Critical,
    /// Important: affects behavior
    Important,
    /// Minor: cosmetic or metadata
    Minor,
    /// Trivial: whitespace, formatting
    Trivial,
}

impl DiffSignificance {
    pub fn emoji(&self) -> &'static str {
        match self {
            DiffSignificance::Critical => "🔴",
            DiffSignificance::Important => "🟠",
            DiffSignificance::Minor => "🟡",
            DiffSignificance::Trivial => "⚪",
        }
    }
}

/// Overall sync status between import source and canonical skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SyncStatus {
    /// In sync - no changes detected
    InSync,
    /// Source is newer - import would update canonical
    SourceNewer,
    /// Canonical is newer - export would update source
    CanonicalNewer,
    /// Both changed - merge required
    Diverged,
    /// No previous sync data available
    Unknown,
}

impl SyncStatus {
    pub fn emoji(&self) -> &'static str {
        match self {
            SyncStatus::InSync => "✅",
            SyncStatus::SourceNewer => "⬇️",
            SyncStatus::CanonicalNewer => "⬆️",
            SyncStatus::Diverged => "⚠️",
            SyncStatus::Unknown => "❓",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            SyncStatus::InSync => "In sync - no changes",
            SyncStatus::SourceNewer => "Source is newer - import recommended",
            SyncStatus::CanonicalNewer => "Canonical is newer - export recommended",
            SyncStatus::Diverged => "Both changed - manual merge required",
            SyncStatus::Unknown => "No sync history available",
        }
    }
}

/// Complete sync analysis result
#[derive(Debug, Clone, Serialize)]
pub struct SyncAnalysis {
    /// Overall sync status
    pub status: SyncStatus,
    /// Field-level diffs
    pub diffs: Vec<FieldDiff>,
    /// Current fingerprint (from imported skill)
    pub current_fingerprint: SkillFingerprint,
    /// Previous fingerprint (from sync metadata if available)
    pub previous_fingerprint: Option<SkillFingerprint>,
    /// Sync recommendation
    pub recommendation: SyncRecommendation,
}

/// Recommended sync action
#[derive(Debug, Clone, Serialize)]
pub struct SyncRecommendation {
    pub action: SyncAction,
    pub description: String,
    pub commands: Vec<String>,
}

/// Sync action to take
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SyncAction {
    /// No action needed
    None,
    /// Re-import from source
    Import,
    /// Re-export to source
    Export,
    /// Manual merge required
    Merge,
    /// Initialize sync tracking
    Initialize,
}

/// Sync metadata stored with imported skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMetadata {
    /// Source file path
    pub source_path: String,
    /// Source format
    pub source_format: String,
    /// Fingerprint at last sync
    pub fingerprint: SkillFingerprint,
    /// Last sync timestamp
    pub last_sync: String,
    /// Sync direction (import or export)
    pub direction: String,
}

/// Compare two imported skills and generate diffs
pub fn compare_skills(original: &ImportedSkill, current: &ImportedSkill) -> Vec<FieldDiff> {
    let mut diffs = Vec::new();

    // Name
    if original.name.value != current.name.value {
        diffs.push(FieldDiff {
            field: "name".to_string(),
            change_type: ChangeType::Modified,
            original_value: Some(original.name.value.clone()),
            current_value: Some(current.name.value.clone()),
            significance: DiffSignificance::Critical,
        });
    }

    // Description
    if original.description.value != current.description.value {
        diffs.push(FieldDiff {
            field: "description".to_string(),
            change_type: ChangeType::Modified,
            original_value: Some(truncate_for_diff(&original.description.value, 100)),
            current_value: Some(truncate_for_diff(&current.description.value, 100)),
            significance: DiffSignificance::Minor,
        });
    }

    // Version
    if original.version.value != current.version.value {
        diffs.push(FieldDiff {
            field: "version".to_string(),
            change_type: ChangeType::Modified,
            original_value: Some(original.version.value.clone()),
            current_value: Some(current.version.value.clone()),
            significance: DiffSignificance::Important,
        });
    }

    // Instructions
    if original.instructions_content.value != current.instructions_content.value {
        let orig_len = original.instructions_content.value.len();
        let curr_len = current.instructions_content.value.len();
        let diff_pct = if orig_len > 0 {
            ((curr_len as i64 - orig_len as i64).abs() * 100 / orig_len as i64) as u32
        } else {
            100
        };

        diffs.push(FieldDiff {
            field: "instructions".to_string(),
            change_type: ChangeType::Modified,
            original_value: Some(format!("{} chars", orig_len)),
            current_value: Some(format!("{} chars ({}% change)", curr_len, diff_pct)),
            significance: if diff_pct > 20 {
                DiffSignificance::Important
            } else {
                DiffSignificance::Minor
            },
        });
    }

    // Daemons
    let orig_daemon_names: Vec<_> = original.daemons.iter().map(|d| &d.name.value).collect();
    let curr_daemon_names: Vec<_> = current.daemons.iter().map(|d| &d.name.value).collect();

    for daemon in &current.daemons {
        if !orig_daemon_names.contains(&&daemon.name.value) {
            diffs.push(FieldDiff {
                field: format!("daemon.{}", daemon.name.value),
                change_type: ChangeType::Added,
                original_value: None,
                current_value: Some(format!("{} methods", daemon.methods.len())),
                significance: DiffSignificance::Critical,
            });
        }
    }

    for daemon in &original.daemons {
        if !curr_daemon_names.contains(&&daemon.name.value) {
            diffs.push(FieldDiff {
                field: format!("daemon.{}", daemon.name.value),
                change_type: ChangeType::Removed,
                original_value: Some(format!("{} methods", daemon.methods.len())),
                current_value: None,
                significance: DiffSignificance::Critical,
            });
        }
    }

    // Compare methods for matching daemons
    for orig_daemon in &original.daemons {
        if let Some(curr_daemon) = current.daemons.iter().find(|d| d.name.value == orig_daemon.name.value) {
            let orig_methods: Vec<_> = orig_daemon.methods.iter().map(|m| &m.value).collect();
            let curr_methods: Vec<_> = curr_daemon.methods.iter().map(|m| &m.value).collect();

            for method in &curr_methods {
                if !orig_methods.contains(method) {
                    diffs.push(FieldDiff {
                        field: format!("{}.{}", orig_daemon.name.value, method),
                        change_type: ChangeType::Added,
                        original_value: None,
                        current_value: Some("new method".to_string()),
                        significance: DiffSignificance::Important,
                    });
                }
            }

            for method in &orig_methods {
                if !curr_methods.contains(method) {
                    diffs.push(FieldDiff {
                        field: format!("{}.{}", orig_daemon.name.value, method),
                        change_type: ChangeType::Removed,
                        original_value: Some("removed".to_string()),
                        current_value: None,
                        significance: DiffSignificance::Important,
                    });
                }
            }
        }
    }

    // Triggers
    let orig_keywords: Vec<_> = original.triggers.keywords.iter().map(|k| &k.value).collect();
    let curr_keywords: Vec<_> = current.triggers.keywords.iter().map(|k| &k.value).collect();

    let added_kw: Vec<_> = curr_keywords.iter().filter(|k| !orig_keywords.contains(k)).collect();
    let removed_kw: Vec<_> = orig_keywords.iter().filter(|k| !curr_keywords.contains(k)).collect();

    if !added_kw.is_empty() {
        diffs.push(FieldDiff {
            field: "triggers.keywords".to_string(),
            change_type: ChangeType::Added,
            original_value: None,
            current_value: Some(format!("+{} keywords", added_kw.len())),
            significance: DiffSignificance::Minor,
        });
    }

    if !removed_kw.is_empty() {
        diffs.push(FieldDiff {
            field: "triggers.keywords".to_string(),
            change_type: ChangeType::Removed,
            original_value: Some(format!("-{} keywords", removed_kw.len())),
            current_value: None,
            significance: DiffSignificance::Minor,
        });
    }

    diffs
}

/// Analyze sync status by comparing current import with existing canonical skill
pub fn analyze_sync(
    imported: &ImportedSkill,
    canonical_path: Option<&Path>,
) -> SyncAnalysis {
    let current_fingerprint = SkillFingerprint::from_imported(imported);

    // Try to load sync metadata from canonical skill directory
    let previous_fingerprint: Option<SkillFingerprint> = if let Some(path) = canonical_path {
        let sync_path = path.join(".sync.json");
        if sync_path.exists() {
            match fs::read_to_string(&sync_path) {
                Ok(content) => {
                    match serde_json::from_str::<SyncMetadata>(&content) {
                        Ok(metadata) => Some(metadata.fingerprint),
                        Err(_) => None,
                    }
                }
                Err(_) => None,
            }
        } else {
            None
        }
    } else {
        None
    };

    // Determine sync status
    let (status, diffs) = match &previous_fingerprint {
        Some(prev) => {
            if prev.combined_hash == current_fingerprint.combined_hash {
                (SyncStatus::InSync, Vec::new())
            } else {
                // Hashes differ - determine what changed
                let mut field_diffs = Vec::new();

                if prev.name_hash != current_fingerprint.name_hash {
                    field_diffs.push(FieldDiff {
                        field: "name".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[previous]".to_string()),
                        current_value: Some(imported.name.value.clone()),
                        significance: DiffSignificance::Critical,
                    });
                }

                if prev.description_hash != current_fingerprint.description_hash {
                    field_diffs.push(FieldDiff {
                        field: "description".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[changed]".to_string()),
                        current_value: Some(truncate_for_diff(&imported.description.value, 50)),
                        significance: DiffSignificance::Minor,
                    });
                }

                if prev.version_hash != current_fingerprint.version_hash {
                    field_diffs.push(FieldDiff {
                        field: "version".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[previous version]".to_string()),
                        current_value: Some(imported.version.value.clone()),
                        significance: DiffSignificance::Important,
                    });
                }

                if prev.daemons_hash != current_fingerprint.daemons_hash {
                    field_diffs.push(FieldDiff {
                        field: "daemons".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[changed]".to_string()),
                        current_value: Some(format!("{} daemons", imported.daemons.len())),
                        significance: DiffSignificance::Critical,
                    });
                }

                if prev.instructions_hash != current_fingerprint.instructions_hash {
                    field_diffs.push(FieldDiff {
                        field: "instructions".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[changed]".to_string()),
                        current_value: Some(format!("{} chars", imported.instructions_content.value.len())),
                        significance: DiffSignificance::Important,
                    });
                }

                if prev.triggers_hash != current_fingerprint.triggers_hash {
                    field_diffs.push(FieldDiff {
                        field: "triggers".to_string(),
                        change_type: ChangeType::Modified,
                        original_value: Some("[changed]".to_string()),
                        current_value: Some(format!("{} triggers",
                            imported.triggers.keywords.len() + imported.triggers.patterns.len())),
                        significance: DiffSignificance::Minor,
                    });
                }

                // For now, assume source is newer since we're importing
                (SyncStatus::SourceNewer, field_diffs)
            }
        }
        None => {
            // No previous sync - this is a fresh import
            (SyncStatus::Unknown, Vec::new())
        }
    };

    // Generate recommendation
    let recommendation = match status {
        SyncStatus::InSync => SyncRecommendation {
            action: SyncAction::None,
            description: "Skill is in sync with source. No action needed.".to_string(),
            commands: vec![],
        },
        SyncStatus::SourceNewer => SyncRecommendation {
            action: SyncAction::Import,
            description: "Source has been updated. Re-import to update canonical skill.".to_string(),
            commands: vec![
                format!("fgp skill import {} --output {}",
                    imported.source_path.display(),
                    canonical_path.map(|p| p.display().to_string()).unwrap_or_else(|| "./".to_string())
                ),
            ],
        },
        SyncStatus::CanonicalNewer => SyncRecommendation {
            action: SyncAction::Export,
            description: "Canonical skill has been updated. Re-export to update source.".to_string(),
            commands: vec![
                format!("fgp skill export {} {} --output {}",
                    imported.source_format.to_key(),
                    canonical_path.map(|p| p.display().to_string()).unwrap_or_else(|| "./".to_string()),
                    imported.source_path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| ".".to_string())
                ),
            ],
        },
        SyncStatus::Diverged => SyncRecommendation {
            action: SyncAction::Merge,
            description: "Both source and canonical have changed. Manual merge required.".to_string(),
            commands: vec![
                "# Compare and manually merge changes".to_string(),
                format!("diff {} {}",
                    imported.source_path.display(),
                    canonical_path.map(|p| p.join("skill.yaml").display().to_string()).unwrap_or_else(|| "./skill.yaml".to_string())
                ),
            ],
        },
        SyncStatus::Unknown => SyncRecommendation {
            action: SyncAction::Initialize,
            description: "No sync history. Import will initialize sync tracking.".to_string(),
            commands: vec![
                format!("fgp skill import {} --output ./",
                    imported.source_path.display()
                ),
            ],
        },
    };

    SyncAnalysis {
        status,
        diffs,
        current_fingerprint,
        previous_fingerprint,
        recommendation,
    }
}

/// Generate sync metadata JSON for storage
pub fn generate_sync_metadata(skill: &ImportedSkill) -> String {
    let metadata = SyncMetadata {
        source_path: skill.source_path.display().to_string(),
        source_format: skill.source_format.to_key().to_string(),
        fingerprint: SkillFingerprint::from_imported(skill),
        last_sync: chrono::Utc::now().to_rfc3339(),
        direction: "import".to_string(),
    };

    serde_json::to_string_pretty(&metadata).unwrap_or_else(|_| "{}".to_string())
}

/// Truncate string for diff display
fn truncate_for_diff(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

// ============================================================================
// Shared Markdown Parsing Utilities
// ============================================================================

/// Extract the first H1 header from markdown content
fn extract_first_h1(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return Some(trimmed[2..].trim().to_string());
        }
    }
    None
}

/// Extract a role name from an intro line like "You are an AI assistant specialized in X"
fn extract_role_name(line: &str) -> Option<String> {
    let line_lower = line.to_lowercase();

    // Pattern: "You are an AI assistant specialized in X"
    if line_lower.contains("specialized in") || line_lower.contains("specializing in") {
        if let Some(idx) = line_lower.find("specialized in").or_else(|| line_lower.find("specializing in")) {
            let after = &line[idx..];
            // Skip "specialized in " or "specializing in "
            let start = if after.to_lowercase().starts_with("specialized in") { 15 } else { 16 };
            if after.len() > start {
                let role_part = after[start..].trim();
                // Take until period or end
                let role = role_part.split('.').next().unwrap_or(role_part).trim();
                if !role.is_empty() && role.len() < 50 {
                    // Convert to title case slug
                    return Some(role.split_whitespace()
                        .map(|w| {
                            let mut c = w.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" "));
                }
            }
        }
    }

    // Pattern: "X Assistant" or "X Manager" or "X Helper"
    let assistant_re = Regex::new(r"(?i)(\w+(?:\s+\w+)?)\s+(?:assistant|manager|helper|agent|bot)").ok()?;
    if let Some(cap) = assistant_re.captures(line) {
        let name = cap[1].trim();
        if !["an", "ai", "the", "a", "your"].contains(&name.to_lowercase().as_str()) {
            return Some(format!("{} Assistant", name));
        }
    }

    None
}

/// Extract content under a specific section header (## or ###)
fn extract_section_content(content: &str, section_names: &[&str]) -> Option<String> {
    let lines: Vec<&str> = content.lines().collect();
    let mut in_section = false;
    let mut section_content = Vec::new();
    let section_level_re = Regex::new(r"^(#{1,3})\s+").unwrap();

    for line in lines {
        let trimmed = line.trim();

        // Check if this is a header that matches our target sections
        if let Some(caps) = section_level_re.captures(trimmed) {
            let header_level = caps[1].len();
            let header_text = trimmed[caps[0].len()..].trim().to_lowercase();

            let matches = section_names
                .iter()
                .any(|&name| header_text.contains(&name.to_lowercase()));

            if matches {
                in_section = true;
                continue;
            } else if in_section && header_level <= 2 {
                // Stop at same or higher level header
                break;
            }
        }

        if in_section {
            section_content.push(line);
        }
    }

    if section_content.is_empty() {
        None
    } else {
        Some(section_content.join("\n").trim().to_string())
    }
}

/// Extract list items from markdown content
fn extract_list_items(content: &str) -> Vec<String> {
    let mut items = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = trimmed[2..].trim().to_string();
            if !item.is_empty() {
                items.push(item);
            }
        } else if let Some(rest) = trimmed.strip_prefix(char::is_numeric) {
            // Numbered list: 1. item
            if let Some(item) = rest.strip_prefix(". ") {
                items.push(item.trim().to_string());
            }
        }
    }
    items
}

/// Extract skill name from path (directory name or filename without extension)
fn extract_name_from_path(path: &Path) -> String {
    // Try parent directory first (common convention)
    if let Some(parent) = path.parent() {
        if let Some(name) = parent.file_name() {
            let name_str = name.to_string_lossy();
            // Skip generic directory names
            if !["skills", "claude-code", "cursorrules", "rules", "."].contains(&name_str.as_ref())
            {
                return name_str.to_string();
            }
        }
    }

    // Fall back to filename without extension
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| {
            // Remove common prefixes/suffixes
            s.trim_start_matches('.')
                .trim_end_matches(".cursorrules")
                .trim_end_matches(".rules")
                .trim_end_matches(".windsurf")
                .to_string()
        })
        .unwrap_or_else(|| "unknown-skill".to_string())
}

// ============================================================================
// Claude Code SKILL.md Parser
// ============================================================================

/// YAML frontmatter extracted from SKILL.md
#[derive(Debug, Deserialize)]
struct ClaudeCodeFrontmatter {
    name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    triggers: Vec<String>,
}

/// Parse a Claude Code SKILL.md file
fn parse_claude_code(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Extract YAML frontmatter
    let (frontmatter, body) = extract_yaml_frontmatter(content)?;

    // Parse frontmatter YAML
    let fm: ClaudeCodeFrontmatter = if !frontmatter.is_empty() {
        serde_yaml::from_str(&frontmatter).unwrap_or(ClaudeCodeFrontmatter {
            name: None,
            description: None,
            version: None,
            tools: vec![],
            triggers: vec![],
        })
    } else {
        ClaudeCodeFrontmatter {
            name: None,
            description: None,
            version: None,
            tools: vec![],
            triggers: vec![],
        }
    };

    // Extract name from frontmatter or filename
    let name = if let Some(n) = fm.name {
        ImportedField::high(n, FieldSource::Frontmatter)
    } else {
        // Try to extract from path
        let filename = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|f| f.to_str())
            .unwrap_or("unknown-skill");
        ImportedField::medium(filename.to_string(), FieldSource::Filename)
            .with_note("Inferred from directory name")
    };

    // Extract description
    let description = if let Some(d) = fm.description {
        ImportedField::high(d, FieldSource::Frontmatter)
    } else {
        // Try to extract from first paragraph
        let first_para = extract_first_paragraph(&body);
        if !first_para.is_empty() {
            ImportedField::medium(first_para, FieldSource::Content)
                .with_note("Extracted from first paragraph")
        } else {
            ImportedField::low(
                format!("{} skill", name.value),
                FieldSource::Default,
            )
        }
    };

    // Version (usually not in exports)
    let version = if let Some(v) = fm.version {
        ImportedField::high(v, FieldSource::Frontmatter)
    } else {
        ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update")
    };

    // Extract daemons from tools list
    let daemons = extract_daemons_from_tools(&fm.tools, &body);

    // Extract triggers
    let triggers = extract_triggers(&fm.triggers, &body);

    Ok(ImportedSkill {
        name,
        version,
        description,
        author: None,
        daemons,
        instructions_content: ImportedField::high(body, FieldSource::Content),
        triggers,
        source_format: ImportFormat::ClaudeCode,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

/// Extract YAML frontmatter from markdown content
fn extract_yaml_frontmatter(content: &str) -> Result<(String, String)> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0].trim() != "---" {
        return Ok((String::new(), content.to_string()));
    }

    // Find closing ---
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }

    match end_idx {
        Some(idx) => {
            let frontmatter = lines[1..idx].join("\n");
            let body = lines[idx + 1..].join("\n").trim_start().to_string();
            Ok((frontmatter, body))
        }
        None => Ok((String::new(), content.to_string())),
    }
}

/// Extract first paragraph from markdown body
fn extract_first_paragraph(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut para = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        // Skip headers and empty lines at start
        if para.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
            continue;
        }
        // Stop at empty line or header
        if !para.is_empty() && (trimmed.is_empty() || trimmed.starts_with('#')) {
            break;
        }
        // Skip if it looks like a list item
        if trimmed.starts_with('-') || trimmed.starts_with('*') {
            if para.is_empty() {
                continue;
            } else {
                break;
            }
        }
        para.push(trimmed);
    }

    para.join(" ").trim().to_string()
}

/// Check if a name looks like a valid daemon name (not a file extension or common word)
fn is_valid_daemon_name(name: &str) -> bool {
    // Common file extensions and non-daemon names to filter out
    let invalid_names = [
        "json", "yaml", "yml", "md", "txt", "toml", "xml", "html", "css", "js", "ts",
        "py", "rs", "go", "java", "sh", "bash", "zsh", "env", "log", "tmp", "bak",
        "credentials", "config", "settings", "example", "test", "spec", "mock",
        "src", "bin", "lib", "pkg", "dist", "build", "target", "node_modules",
    ];

    if invalid_names.contains(&name.to_lowercase().as_str()) {
        return false;
    }

    // Must be at least 2 chars
    if name.len() < 2 {
        return false;
    }

    true
}

/// Extract daemons from tools list and body content
fn extract_daemons_from_tools(tools: &[String], body: &str) -> Vec<ImportedDaemon> {
    let mut daemons: HashMap<String, Vec<ImportedField<String>>> = HashMap::new();

    // Parse tools list (e.g., "gmail.inbox", "gmail.send")
    for tool in tools {
        if let Some((daemon_name, method_name)) = tool.split_once('.') {
            if is_valid_daemon_name(daemon_name) {
                daemons
                    .entry(daemon_name.to_string())
                    .or_default()
                    .push(ImportedField::high(
                        method_name.to_string(),
                        FieldSource::Frontmatter,
                    ));
            }
        }
    }

    // Also scan body for fgp call patterns
    let fgp_call_re = Regex::new(r"fgp\s+call\s+(\w+)\.(\w+)").unwrap();
    for cap in fgp_call_re.captures_iter(body) {
        let daemon_name = cap[1].to_string();
        let method_name = cap[2].to_string();

        if !is_valid_daemon_name(&daemon_name) {
            continue;
        }

        let methods = daemons.entry(daemon_name).or_default();
        // Only add if not already present
        if !methods.iter().any(|m| m.value == method_name) {
            methods.push(ImportedField::medium(
                method_name,
                FieldSource::MethodExtraction,
            ));
        }
    }

    // Also scan for fgp-<daemon>-client patterns (e.g., fgp-imessage-client recent)
    let fgp_client_re = Regex::new(r"fgp-(\w+)-client\s+(\w+)").unwrap();
    for cap in fgp_client_re.captures_iter(body) {
        let daemon_name = cap[1].to_string();
        let method_name = cap[2].to_string();

        if !is_valid_daemon_name(&daemon_name) {
            continue;
        }

        let methods = daemons.entry(daemon_name).or_default();
        if !methods.iter().any(|m| m.value == method_name) {
            methods.push(ImportedField::medium(
                method_name,
                FieldSource::MethodExtraction,
            ).with_note("Extracted from fgp-*-client pattern"));
        }
    }

    // Also scan for fgp-<daemon> CLI patterns (e.g., fgp-imessage recent)
    let fgp_cli_re = Regex::new(r"fgp-(\w+)\s+(\w+)(?:\s|$|-)").unwrap();
    for cap in fgp_cli_re.captures_iter(body) {
        let daemon_name = cap[1].to_string();
        let method_name = cap[2].to_string();

        // Skip if it's a client pattern (already handled) or common flags
        if method_name == "client" || method_name == "daemon" {
            continue;
        }

        if !is_valid_daemon_name(&daemon_name) {
            continue;
        }

        let methods = daemons.entry(daemon_name).or_default();
        if !methods.iter().any(|m| m.value == method_name) {
            methods.push(ImportedField::low(
                method_name,
                FieldSource::MethodExtraction,
            ).with_note("Extracted from fgp-* CLI pattern"));
        }
    }

    // Scan for daemon.method patterns in markdown bullet lists
    // Matches: "- gmail.inbox - description" or "- `gmail.inbox` - description"
    // Also matches: "**gmail.inbox** - description"
    let bullet_re = Regex::new(r"[-*]\s+[`*]*(\w+)\.(\w+)[`*]*\s*[-–—:]").unwrap();
    for cap in bullet_re.captures_iter(body) {
        let daemon_name = cap[1].to_string();
        let method_name = cap[2].to_string();

        if !is_valid_daemon_name(&daemon_name) {
            continue;
        }

        let methods = daemons.entry(daemon_name).or_default();
        if !methods.iter().any(|m| m.value == method_name) {
            methods.push(ImportedField::medium(
                method_name,
                FieldSource::Content,
            ).with_note("Extracted from markdown bullet list"));
        }
    }

    // Also look for method tables
    let table_methods = extract_methods_from_tables(body);
    for (daemon_name, method_names) in table_methods {
        if !is_valid_daemon_name(&daemon_name) {
            continue;
        }
        let methods = daemons.entry(daemon_name).or_default();
        for method_name in method_names {
            if !methods.iter().any(|m| m.value == method_name) {
                methods.push(ImportedField::medium(method_name, FieldSource::Content));
            }
        }
    }

    // Convert to ImportedDaemon
    daemons
        .into_iter()
        .map(|(name, methods)| {
            let confidence = if methods.iter().any(|m| m.confidence == Confidence::High) {
                Confidence::High
            } else {
                Confidence::Medium
            };

            ImportedDaemon {
                name: ImportedField {
                    value: name,
                    confidence,
                    source: FieldSource::MethodExtraction,
                    notes: None,
                },
                version: ImportedField::low(
                    Some(">=1.0.0".to_string()),
                    FieldSource::Default,
                )
                .with_note("Default version constraint"),
                optional: ImportedField::low(false, FieldSource::Default),
                methods,
            }
        })
        .collect()
}

/// Extract method names from markdown tables
fn extract_methods_from_tables(body: &str) -> HashMap<String, Vec<String>> {
    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    // Look for tables with "Method" header
    let table_re = Regex::new(r"\|\s*Method\s*\|").unwrap();
    if !table_re.is_match(body) {
        return result;
    }

    // Find method patterns like `daemon.method` or just `method`
    let method_re = Regex::new(r"`(\w+)\.(\w+)`").unwrap();
    for cap in method_re.captures_iter(body) {
        let daemon_name = cap[1].to_string();
        let method_name = cap[2].to_string();
        result.entry(daemon_name).or_default().push(method_name);
    }

    result
}

/// Extract triggers from frontmatter and body
fn extract_triggers(frontmatter_triggers: &[String], body: &str) -> ImportedTriggers {
    let mut triggers = ImportedTriggers::default();

    // Add frontmatter triggers as keywords
    for trigger in frontmatter_triggers {
        triggers.keywords.push(ImportedField::high(
            trigger.clone(),
            FieldSource::Frontmatter,
        ));
    }

    // Look for trigger sections in body
    let trigger_section_re = Regex::new(r"(?i)##?\s*(triggers?|when to use|activation)").unwrap();
    if trigger_section_re.is_match(body) {
        // Extract list items after the trigger section
        let lines: Vec<&str> = body.lines().collect();
        let mut in_trigger_section = false;

        for line in lines {
            let trimmed = line.trim();

            if trigger_section_re.is_match(trimmed) {
                in_trigger_section = true;
                continue;
            }

            if in_trigger_section {
                // Stop at next header
                if trimmed.starts_with('#') {
                    break;
                }

                // Extract list items
                if trimmed.starts_with('-') || trimmed.starts_with('*') {
                    let item = trimmed
                        .trim_start_matches('-')
                        .trim_start_matches('*')
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();

                    if !item.is_empty()
                        && !triggers.keywords.iter().any(|k| k.value == item)
                    {
                        triggers.keywords.push(ImportedField::medium(
                            item,
                            FieldSource::Content,
                        ));
                    }
                }
            }
        }
    }

    // Look for /command patterns (must be at word boundary, not in paths or dates)
    // Match /command at start of line or after whitespace, not numbers or path segments
    let command_re = Regex::new(r"(?:^|\s)/([a-zA-Z][a-zA-Z0-9-]*)(?:\s|$)").unwrap();
    for cap in command_re.captures_iter(body) {
        let cmd = format!("/{}", &cap[1]);
        // Skip common false positives
        if cmd.len() < 3 {
            continue;
        }
        if !triggers.commands.iter().any(|c| c.value == cmd) {
            triggers.commands.push(ImportedField::medium(
                cmd,
                FieldSource::Content,
            ));
        }
    }

    triggers
}

// ============================================================================
// Cursor .cursorrules Parser
// ============================================================================

/// Parse a Cursor .cursorrules file (pure markdown, no frontmatter)
fn parse_cursor(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Extract name from first H1 or directory name
    let name = if let Some(h1) = extract_first_h1(content) {
        ImportedField::medium(h1, FieldSource::Content)
            .with_note("Extracted from first H1 header")
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract description from first paragraph
    let description = {
        let first_para = extract_first_paragraph(content);
        if !first_para.is_empty() {
            ImportedField::medium(first_para, FieldSource::Content)
                .with_note("Extracted from first paragraph")
        } else {
            ImportedField::low(
                format!("{} skill", name.value),
                FieldSource::Default,
            )
        }
    };

    // Extract daemons from content patterns
    let daemons = extract_daemons_from_tools(&[], content);

    // Extract triggers from content
    let triggers = extract_triggers(&[], content);

    Ok(ImportedSkill {
        name,
        version: ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update"),
        description,
        author: None,
        daemons,
        instructions_content: ImportedField::high(content.to_string(), FieldSource::Content),
        triggers,
        source_format: ImportFormat::Cursor,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// Zed .rules Parser
// ============================================================================

/// Parse a Zed .rules file (markdown format)
fn parse_zed(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Extract name - try multiple strategies
    let name = if let Some(h1) = extract_first_h1(content) {
        ImportedField::medium(h1, FieldSource::Content)
            .with_note("Extracted from first H1 header")
    } else {
        // Try to extract a meaningful name from the first line if it describes a role
        let first_line = content.lines().next().unwrap_or("").trim();
        let role_name = extract_role_name(first_line);
        if let Some(role) = role_name {
            ImportedField::medium(role, FieldSource::Content)
                .with_note("Extracted from role description")
        } else {
            // Use filename without extension
            let filename = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("skill");
            // Convert "project" or generic names to something more meaningful
            if filename == "project" || filename == "rules" {
                // Try to get project name from parent directory
                let parent_name = path.parent()
                    .and_then(|p| p.file_name())
                    .and_then(|s| s.to_str())
                    .unwrap_or("skill");
                ImportedField::low(parent_name.to_string(), FieldSource::Filename)
                    .with_note("Inferred from parent directory")
            } else {
                ImportedField::low(filename.to_string(), FieldSource::Filename)
                    .with_note("Inferred from filename")
            }
        }
    };

    // Extract description - first paragraph or first meaningful sentence
    let description = {
        let first_para = extract_first_paragraph(content);
        if !first_para.is_empty() {
            ImportedField::medium(first_para, FieldSource::Content)
                .with_note("Extracted from first paragraph")
        } else {
            // Try first line as description
            let first_line = content.lines().next().unwrap_or("").trim();
            if !first_line.is_empty() && first_line.len() < 200 {
                ImportedField::medium(first_line.to_string(), FieldSource::Content)
                    .with_note("Extracted from first line")
            } else {
                ImportedField::low(
                    format!("{} skill", name.value),
                    FieldSource::Default,
                )
            }
        }
    };

    // Extract daemons from content patterns
    let daemons = extract_daemons_from_tools(&[], content);

    // Extract triggers - also look for keyword sections
    let mut triggers = extract_triggers(&[], content);

    // If no triggers found, try to extract from daemon names and section headers
    if triggers.keywords.is_empty() {
        let mut keywords = Vec::new();

        // Add daemon names as keywords
        for daemon in &daemons {
            keywords.push(ImportedField::low(
                daemon.name.value.clone(),
                FieldSource::MethodExtraction,
            ).with_note("Inferred from daemon name"));
        }

        // Look for section headers that might indicate capabilities
        let header_re = Regex::new(r"##\s+(.+?)(?:\s*\(|$)").unwrap();
        for cap in header_re.captures_iter(content) {
            let header = cap[1].trim().to_lowercase();
            // Skip generic headers
            if !["guidelines", "code style", "project structure", "available tools",
                 "best practices", "configuration", "example", "usage"].contains(&header.as_str()) {
                if header.len() < 30 {
                    keywords.push(ImportedField::low(
                        header,
                        FieldSource::Content,
                    ).with_note("Extracted from section header"));
                }
            }
        }

        triggers.keywords = keywords;
    }

    Ok(ImportedSkill {
        name,
        version: ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update"),
        description,
        author: None,
        daemons,
        instructions_content: ImportedField::high(content.to_string(), FieldSource::Content),
        triggers,
        source_format: ImportFormat::Zed,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// Windsurf .windsurf.md Parser
// ============================================================================

/// Parse a Windsurf .windsurf.md file (markdown with optional YAML frontmatter)
fn parse_windsurf(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Windsurf may have frontmatter
    let (frontmatter, body) = extract_yaml_frontmatter(content)?;

    // Parse frontmatter if present - full structure with capabilities and triggers
    #[derive(Debug, Deserialize, Default)]
    struct WindsurfCapability {
        name: Option<String>,
        #[serde(default)]
        tools: Vec<String>,
    }

    #[derive(Debug, Deserialize, Default)]
    struct WindsurfTriggers {
        #[serde(default)]
        keywords: Vec<String>,
        #[serde(default)]
        patterns: Vec<String>,
    }

    #[derive(Debug, Deserialize, Default)]
    struct WindsurfFrontmatter {
        name: Option<String>,
        version: Option<String>,
        description: Option<String>,
        author: Option<String>,
        #[serde(default)]
        capabilities: Vec<WindsurfCapability>,
        triggers: Option<WindsurfTriggers>,
    }

    let fm: WindsurfFrontmatter = if !frontmatter.is_empty() {
        serde_yaml::from_str(&frontmatter).unwrap_or_default()
    } else {
        WindsurfFrontmatter::default()
    };

    // Extract name
    let name = if let Some(n) = fm.name {
        ImportedField::high(n, FieldSource::Frontmatter)
    } else if let Some(h1) = extract_first_h1(&body) {
        ImportedField::medium(h1, FieldSource::Content)
            .with_note("Extracted from first H1 header")
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract version from frontmatter
    let version = if let Some(v) = fm.version {
        ImportedField::high(v, FieldSource::Frontmatter)
    } else {
        ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update")
    };

    // Extract description
    let description = if let Some(d) = fm.description {
        ImportedField::high(d, FieldSource::Frontmatter)
    } else {
        let first_para = extract_first_paragraph(&body);
        if !first_para.is_empty() {
            ImportedField::medium(first_para, FieldSource::Content)
                .with_note("Extracted from first paragraph")
        } else {
            ImportedField::low(
                format!("{} skill", name.value),
                FieldSource::Default,
            )
        }
    };

    // Extract author from frontmatter
    let author = fm.author.map(|a| {
        ImportedAuthor {
            name: ImportedField::high(a, FieldSource::Frontmatter),
            email: ImportedField::low(None, FieldSource::Default),
            url: ImportedField::low(None, FieldSource::Default),
        }
    });

    // Extract daemons from capabilities in frontmatter
    let mut daemons: Vec<ImportedDaemon> = Vec::new();

    // First, collect tools from capabilities
    let mut all_tools: Vec<String> = Vec::new();
    for cap in &fm.capabilities {
        all_tools.extend(cap.tools.iter().cloned());
    }

    // Parse tools into daemon.method pairs
    if !all_tools.is_empty() {
        let mut daemon_methods: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

        for tool in &all_tools {
            if let Some((daemon, method)) = tool.split_once('.') {
                daemon_methods
                    .entry(daemon.to_string())
                    .or_default()
                    .push(method.to_string());
            }
        }

        for (daemon_name, methods) in daemon_methods {
            daemons.push(ImportedDaemon {
                name: ImportedField::high(daemon_name, FieldSource::Frontmatter),
                version: ImportedField::low(Some(">=1.0.0".to_string()), FieldSource::Default),
                optional: ImportedField::low(false, FieldSource::Default),
                methods: methods.into_iter()
                    .map(|m| ImportedField::high(m, FieldSource::Frontmatter))
                    .collect(),
            });
        }
    }

    // If no daemons from frontmatter, try extracting from content
    if daemons.is_empty() {
        daemons = extract_daemons_from_tools(&[], &body);
    }

    // Extract triggers from frontmatter or content
    let triggers = if let Some(t) = fm.triggers {
        let keywords: Vec<ImportedField<String>> = t.keywords.into_iter()
            .map(|k| ImportedField::high(k, FieldSource::Frontmatter))
            .collect();
        let patterns: Vec<ImportedField<String>> = t.patterns.into_iter()
            .map(|p| ImportedField::high(p, FieldSource::Frontmatter))
            .collect();

        // If we have triggers from frontmatter, use them
        if !keywords.is_empty() || !patterns.is_empty() {
            ImportedTriggers {
                keywords,
                patterns,
                commands: Vec::new(),
            }
        } else {
            extract_triggers(&[], &body)
        }
    } else {
        extract_triggers(&[], &body)
    };

    Ok(ImportedSkill {
        name,
        version,
        description,
        author,
        daemons,
        instructions_content: ImportedField::high(body, FieldSource::Content),
        triggers,
        source_format: ImportFormat::Windsurf,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// Aider .CONVENTIONS.md Parser
// ============================================================================

/// Parse an Aider .CONVENTIONS.md file (markdown format)
fn parse_aider(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Extract description first (we may use it for name inference)
    let first_para = extract_first_paragraph(content);
    let description = if let Some(overview) = extract_section_content(content, &["overview", "about", "description"]) {
        let first_line = overview.lines().next().unwrap_or("").trim().to_string();
        if !first_line.is_empty() {
            ImportedField::medium(first_line, FieldSource::Content)
                .with_note("Extracted from Overview section")
        } else if !first_para.is_empty() {
            ImportedField::medium(first_para.clone(), FieldSource::Content)
                .with_note("Extracted from first paragraph")
        } else {
            ImportedField::low("Conventions skill".to_string(), FieldSource::Default)
        }
    } else if !first_para.is_empty() {
        ImportedField::medium(first_para.clone(), FieldSource::Content)
            .with_note("Extracted from first paragraph")
    } else {
        ImportedField::low("Conventions skill".to_string(), FieldSource::Default)
    };

    // Extract name from first H1 or try to infer from description
    let name = if let Some(h1) = extract_first_h1(content) {
        // Often the H1 is just "Conventions" or similar, so check if it's generic
        let lower = h1.to_lowercase();
        if lower.contains("convention") || lower.contains("rules") || lower.contains("guide") {
            // Try to extract a project name from description
            // Look for patterns like "for the X project" or "X assistant"
            if let Some(project_name) = extract_project_name_from_text(&first_para) {
                ImportedField::medium(project_name, FieldSource::Content)
                    .with_note("Inferred from description")
            } else {
                ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
                    .with_note("H1 was generic, using path")
            }
        } else {
            ImportedField::medium(h1, FieldSource::Content)
                .with_note("Extracted from first H1 header")
        }
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract daemons from content patterns
    let daemons = extract_daemons_from_tools(&[], content);

    // Extract triggers - Aider often has "Commands" or "Usage" sections
    let mut triggers = extract_triggers(&[], content);

    // Add daemon names as trigger keywords (like Zed parser)
    for daemon in &daemons {
        let daemon_name = daemon.name.value.clone();
        if !triggers.keywords.iter().any(|k| k.value == daemon_name) {
            triggers.keywords.push(ImportedField::low(
                daemon_name,
                FieldSource::MethodExtraction,
            ).with_note("Inferred from detected daemons"));
        }
    }

    // Also check for Aider-specific command patterns
    if let Some(commands_section) = extract_section_content(content, &["commands", "usage"]) {
        for item in extract_list_items(&commands_section) {
            // Look for command-like patterns
            if item.starts_with('/') || item.starts_with("aider") {
                if !triggers.keywords.iter().any(|k| k.value == item) {
                    triggers.keywords.push(ImportedField::medium(
                        item,
                        FieldSource::Content,
                    ).with_note("From Commands/Usage section"));
                }
            }
        }
    }

    Ok(ImportedSkill {
        name,
        version: ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update"),
        description,
        author: None,
        daemons,
        instructions_content: ImportedField::high(content.to_string(), FieldSource::Content),
        triggers,
        source_format: ImportFormat::Aider,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

/// Extract a project name from text like "for the X project" or "X assistant"
fn extract_project_name_from_text(text: &str) -> Option<String> {
    let text_lower = text.to_lowercase();

    // Pattern: "for the X project"
    if let Some(idx) = text_lower.find("for the ") {
        let start = idx + 8;
        let rest = &text[start..];
        if let Some(end_idx) = rest.to_lowercase().find(" project") {
            let name = rest[..end_idx].trim();
            if !name.is_empty() && name.len() < 50 {
                return Some(title_case(name));
            }
        }
    }

    // Pattern: "X assistant" or "X helper"
    let assistant_re = regex::Regex::new(r"(?i)(\w+(?:\s+\w+)?)\s+(?:assistant|helper|manager|service)").ok()?;
    if let Some(cap) = assistant_re.captures(text) {
        let name = cap.get(1)?.as_str();
        if !name.to_lowercase().contains("this") && !name.to_lowercase().contains("the") {
            return Some(title_case(name));
        }
    }

    None
}

/// Convert text to title case
fn title_case(s: &str) -> String {
    s.split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().chain(chars).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ============================================================================
// Gemini gemini-extension.json Parser
// ============================================================================

/// Gemini extension manifest structure
#[derive(Debug, Deserialize)]
struct GeminiManifest {
    name: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    version: Option<String>,
    author: Option<String>,
    #[serde(default)]
    capabilities: Vec<GeminiCapability>,
    #[serde(default)]
    triggers: GeminiTriggers,
    instructions: Option<String>,
    instructions_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GeminiCapability {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct GeminiTriggers {
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    patterns: Vec<String>,
}

/// Parse a Gemini gemini-extension.json file
fn parse_gemini(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Parse JSON manifest
    let manifest: GeminiManifest = serde_json::from_str(content)
        .with_context(|| "Failed to parse Gemini manifest JSON")?;

    // Extract name
    let name = if let Some(n) = manifest.display_name.or(manifest.name) {
        ImportedField::high(n, FieldSource::Frontmatter)
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract description
    let description = if let Some(d) = manifest.description {
        ImportedField::high(d, FieldSource::Frontmatter)
    } else {
        ImportedField::low(
            format!("{} skill", name.value),
            FieldSource::Default,
        )
    };

    // Extract version
    let version = if let Some(v) = manifest.version {
        ImportedField::high(v, FieldSource::Frontmatter)
    } else {
        ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update")
    };

    // Extract instructions content
    let instructions_content = if let Some(instructions) = manifest.instructions {
        ImportedField::high(instructions, FieldSource::Frontmatter)
    } else if let Some(ref file) = manifest.instructions_file {
        // Try to read the instructions file relative to the manifest
        let instructions_path = path.parent().map(|p| p.join(file));
        if let Some(ref ipath) = instructions_path {
            if ipath.exists() {
                let instructions = fs::read_to_string(ipath)
                    .unwrap_or_else(|_| format!("# {}\n\nInstructions file: {}", name.value, file));
                ImportedField::high(instructions, FieldSource::Content)
            } else {
                ImportedField::low(
                    format!("# {}\n\nInstructions file not found: {}", name.value, file),
                    FieldSource::Default,
                ).with_note("Referenced instructions file not found")
            }
        } else {
            ImportedField::low(
                format!("# {}\n\n[Instructions to be added]", name.value),
                FieldSource::Default,
            )
        }
    } else {
        ImportedField::low(
            format!("# {}\n\n[Instructions to be added]", name.value),
            FieldSource::Default,
        )
    };

    // Extract daemons from capabilities
    // Strategy:
    // 1. If capability name is "daemon.method" format, use that directly
    // 2. Otherwise, try to infer daemon from trigger keywords (e.g., "gmail", "calendar")
    // 3. Match capability names against common method patterns
    let mut daemons: HashMap<String, Vec<ImportedField<String>>> = HashMap::new();

    // Collect potential daemon names from trigger keywords
    let potential_daemons: Vec<String> = manifest.triggers.keywords.iter()
        .filter(|kw| is_valid_daemon_name(kw))
        .cloned()
        .collect();

    // Common method name → daemon mappings for inference
    let method_daemon_hints: HashMap<&str, &str> = [
        // Gmail methods
        ("inbox", "gmail"),
        ("send", "gmail"),
        ("search", "gmail"),
        ("read", "gmail"),
        ("thread", "gmail"),
        ("unread", "gmail"),
        // Calendar methods
        ("list", "calendar"),
        ("create", "calendar"),
        ("update", "calendar"),
        ("delete", "calendar"),
        ("upcoming", "calendar"),
        ("today", "calendar"),
        ("free_slots", "calendar"),
        // GitHub methods
        ("issues", "github"),
        ("prs", "github"),
        ("repos", "github"),
        ("commits", "github"),
        // Browser methods
        ("open", "browser"),
        ("navigate", "browser"),
        ("click", "browser"),
        ("fill", "browser"),
        ("screenshot", "browser"),
        ("snapshot", "browser"),
    ].into_iter().collect();

    for cap in &manifest.capabilities {
        if let Some(ref cap_name) = cap.name {
            // Check if it looks like "daemon.method"
            if let Some((daemon_name, method_name)) = cap_name.split_once('.') {
                if is_valid_daemon_name(daemon_name) {
                    daemons
                        .entry(daemon_name.to_string())
                        .or_default()
                        .push(ImportedField::high(
                            method_name.to_string(),
                            FieldSource::Frontmatter,
                        ));
                    continue;
                }
            }

            // Try to infer daemon from method name hints
            let method_lower = cap_name.to_lowercase();
            if let Some(&daemon_hint) = method_daemon_hints.get(method_lower.as_str()) {
                // Verify the hint matches a potential daemon from triggers
                if potential_daemons.iter().any(|d| d == daemon_hint ||
                    (daemon_hint == "gmail" && d == "email") ||
                    (daemon_hint == "calendar" && (d == "schedule" || d == "meeting"))) {
                    daemons
                        .entry(daemon_hint.to_string())
                        .or_default()
                        .push(ImportedField::medium(
                            cap_name.clone(),
                            FieldSource::MethodExtraction,
                        ).with_note("Daemon inferred from trigger keywords"));
                }
            }
        }
    }

    let daemons_vec: Vec<ImportedDaemon> = daemons
        .into_iter()
        .map(|(name, methods)| ImportedDaemon {
            name: ImportedField::medium(name, FieldSource::MethodExtraction)
                .with_note("Inferred from capabilities and triggers"),
            version: ImportedField::low(
                Some(">=1.0.0".to_string()),
                FieldSource::Default,
            ),
            optional: ImportedField::low(false, FieldSource::Default),
            methods,
        })
        .collect();

    // Extract triggers
    let mut triggers = ImportedTriggers::default();
    for kw in manifest.triggers.keywords {
        triggers.keywords.push(ImportedField::high(kw, FieldSource::Frontmatter));
    }
    for pat in manifest.triggers.patterns {
        triggers.patterns.push(ImportedField::high(pat, FieldSource::Frontmatter));
    }

    // Extract author
    let author = manifest.author.map(|a| ImportedAuthor {
        name: ImportedField::high(a, FieldSource::Frontmatter),
        email: ImportedField::low(None, FieldSource::Default),
        url: ImportedField::low(None, FieldSource::Default),
    });

    Ok(ImportedSkill {
        name,
        version,
        description,
        author,
        daemons: daemons_vec,
        instructions_content,
        triggers,
        source_format: ImportFormat::Gemini,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// Codex .codex.json Parser
// ============================================================================

/// Codex configuration structure
#[derive(Debug, Deserialize)]
struct CodexConfig {
    name: Option<String>,
    description: Option<String>,
    instructions: Option<String>,
    instructions_file: Option<String>,
    #[serde(default)]
    tools: Vec<String>,
}

/// Parse a Codex .codex.json file
fn parse_codex(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Parse JSON config
    let config: CodexConfig = serde_json::from_str(content)
        .with_context(|| "Failed to parse Codex config JSON")?;

    // Extract name
    let name = if let Some(n) = config.name {
        ImportedField::high(n, FieldSource::Frontmatter)
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract description
    let description = if let Some(d) = config.description {
        ImportedField::high(d, FieldSource::Frontmatter)
    } else {
        ImportedField::low(
            format!("{} skill", name.value),
            FieldSource::Default,
        )
    };

    // Extract instructions content
    let instructions_content = if let Some(instructions) = config.instructions {
        ImportedField::high(instructions, FieldSource::Frontmatter)
    } else if let Some(ref file) = config.instructions_file {
        // Try to read the instructions file
        let instructions_path = path.parent().map(|p| p.join(file));
        if let Some(ref ipath) = instructions_path {
            if ipath.exists() {
                let instructions = fs::read_to_string(ipath)
                    .unwrap_or_else(|_| format!("# {}\n\nInstructions file: {}", name.value, file));
                ImportedField::high(instructions, FieldSource::Content)
            } else {
                ImportedField::low(
                    format!("# {}\n\nInstructions file not found: {}", name.value, file),
                    FieldSource::Default,
                ).with_note("Referenced instructions file not found")
            }
        } else {
            ImportedField::low(
                format!("# {}\n\n[Instructions to be added]", name.value),
                FieldSource::Default,
            )
        }
    } else {
        ImportedField::low(
            format!("# {}\n\n[Instructions to be added]", name.value),
            FieldSource::Default,
        )
    };

    // Extract daemons from tools
    let daemons = extract_daemons_from_tools(&config.tools, &instructions_content.value);

    Ok(ImportedSkill {
        name,
        version: ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update"),
        description,
        author: None,
        daemons,
        instructions_content,
        triggers: ImportedTriggers::default(),
        source_format: ImportFormat::Codex,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// MCP .mcp.json Parser
// ============================================================================

/// MCP tool schema structure
#[derive(Debug, Deserialize)]
struct McpConfig {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    tools: Vec<McpTool>,
}

#[derive(Debug, Deserialize)]
struct McpTool {
    name: String,
    description: Option<String>,
}

/// Parse an MCP .mcp.json file
fn parse_mcp(path: &Path, content: &str) -> Result<ImportedSkill> {
    let now = chrono::Utc::now().to_rfc3339();

    // Parse JSON config
    let config: McpConfig = serde_json::from_str(content)
        .with_context(|| "Failed to parse MCP config JSON")?;

    // Extract name
    let name = if let Some(n) = config.name {
        ImportedField::high(n, FieldSource::Frontmatter)
    } else {
        ImportedField::low(extract_name_from_path(path), FieldSource::Filename)
            .with_note("Inferred from path")
    };

    // Extract description
    let description = if let Some(d) = config.description {
        ImportedField::high(d, FieldSource::Frontmatter)
    } else {
        ImportedField::low(
            format!("{} skill", name.value),
            FieldSource::Default,
        )
    };

    // Extract daemons from tools (MCP tools are often "daemon__method" format)
    let mut daemons_map: HashMap<String, Vec<ImportedField<String>>> = HashMap::new();
    for tool in &config.tools {
        // MCP tools are often formatted as "mcp__server__method" or "daemon.method"
        let parts: Vec<&str> = tool.name.split("__").collect();
        if parts.len() >= 2 {
            // Format: mcp__daemon__method or daemon__method
            let (daemon_name, method_name) = if parts[0] == "mcp" && parts.len() >= 3 {
                (parts[1], parts[2])
            } else {
                (parts[0], parts[1])
            };

            if is_valid_daemon_name(daemon_name) {
                daemons_map
                    .entry(daemon_name.to_string())
                    .or_default()
                    .push(ImportedField::high(
                        method_name.to_string(),
                        FieldSource::Frontmatter,
                    ));
            }
        } else if let Some((daemon_name, method_name)) = tool.name.split_once('.') {
            if is_valid_daemon_name(daemon_name) {
                daemons_map
                    .entry(daemon_name.to_string())
                    .or_default()
                    .push(ImportedField::high(
                        method_name.to_string(),
                        FieldSource::Frontmatter,
                    ));
            }
        }
    }

    let daemons: Vec<ImportedDaemon> = daemons_map
        .into_iter()
        .map(|(name, methods)| ImportedDaemon {
            name: ImportedField::high(name, FieldSource::Frontmatter),
            version: ImportedField::low(
                Some(">=1.0.0".to_string()),
                FieldSource::Default,
            ),
            optional: ImportedField::low(false, FieldSource::Default),
            methods,
        })
        .collect();

    // Generate basic instructions from tool descriptions
    let mut instructions = format!("# {}\n\n", name.value);
    instructions.push_str("## Available Tools\n\n");
    for tool in &config.tools {
        instructions.push_str(&format!(
            "- **{}**: {}\n",
            tool.name,
            tool.description.as_deref().unwrap_or("No description")
        ));
    }

    Ok(ImportedSkill {
        name,
        version: ImportedField::low("1.0.0".to_string(), FieldSource::Default)
            .with_note("Default version - please update"),
        description,
        author: None,
        daemons,
        instructions_content: ImportedField::medium(instructions, FieldSource::Content)
            .with_note("Generated from tool list"),
        triggers: ImportedTriggers::default(),
        source_format: ImportFormat::Mcp,
        source_path: path.to_path_buf(),
        import_timestamp: now,
    })
}

// ============================================================================
// Skill.yaml Generator
// ============================================================================

/// Generate skill.yaml content from imported skill
fn generate_skill_yaml(skill: &ImportedSkill) -> String {
    let mut yaml = String::new();

    // Header comment
    yaml.push_str(&format!(
        "# Imported from {} on {}\n",
        skill.source_format.name(),
        &skill.import_timestamp[..10] // Just the date
    ));
    yaml.push_str("# Fields marked [*LOW-CONFIDENCE*] or [*INCOMPLETE*] need review\n\n");

    // Core metadata
    yaml.push_str(&format!("name: {}\n", skill.name.value));
    yaml.push_str(&format!("version: {}", skill.version.value));
    if skill.version.confidence != Confidence::High {
        yaml.push_str("  # [*LOW-CONFIDENCE*] Update version");
    }
    yaml.push('\n');
    yaml.push_str(&format!("description: {}\n", skill.description.value));

    // Author
    yaml.push_str("\nauthor:\n");
    if let Some(ref author) = skill.author {
        yaml.push_str(&format!("  name: \"{}\"\n", author.name.value));
        if author.name.confidence != Confidence::High {
            yaml.push_str("  # [*LOW-CONFIDENCE*] Verify author\n");
        }
        if let Some(ref email) = author.email.value {
            yaml.push_str(&format!("  email: {}\n", email));
        } else {
            yaml.push_str("  # email: author@example.com\n");
        }
    } else {
        yaml.push_str("  name: \"Unknown\"  # [*INCOMPLETE*] Add author name\n");
        yaml.push_str("  # email: author@example.com\n");
    }

    // License
    yaml.push_str("\nlicense: MIT  # [*LOW-CONFIDENCE*] Verify license\n");

    // Daemons
    if !skill.daemons.is_empty() {
        yaml.push_str("\ndaemons:\n");
        for daemon in &skill.daemons {
            yaml.push_str(&format!("  - name: {}\n", daemon.name.value));
            if daemon.name.confidence != Confidence::High {
                yaml.push_str("    # [*LOW-CONFIDENCE*] Verify daemon name\n");
            }
            if let Some(ref ver) = daemon.version.value {
                yaml.push_str(&format!("    version: \"{}\"\n", ver));
            }
            if daemon.optional.value {
                yaml.push_str("    optional: true\n");
            }
            if !daemon.methods.is_empty() {
                yaml.push_str("    methods:\n");
                for method in &daemon.methods {
                    yaml.push_str(&format!("      - {}\n", method.value));
                }
            }
        }
    }

    // Instructions
    yaml.push_str("\ninstructions:\n");
    yaml.push_str("  core: ./instructions/core.md\n");
    yaml.push_str(&format!(
        "  {}: ./instructions/{}.md\n",
        skill.source_format.to_key(),
        skill.source_format.to_key()
    ));

    // Triggers
    if !skill.triggers.keywords.is_empty()
        || !skill.triggers.patterns.is_empty()
        || !skill.triggers.commands.is_empty()
    {
        yaml.push_str("\ntriggers:\n");

        if !skill.triggers.keywords.is_empty() {
            yaml.push_str("  keywords:\n");
            for kw in &skill.triggers.keywords {
                yaml.push_str(&format!("    - \"{}\"\n", kw.value));
            }
        }

        if !skill.triggers.patterns.is_empty() {
            yaml.push_str("  patterns:\n");
            for pat in &skill.triggers.patterns {
                yaml.push_str(&format!("    - \"{}\"\n", pat.value));
            }
        }

        if !skill.triggers.commands.is_empty() {
            yaml.push_str("  commands:\n");
            for cmd in &skill.triggers.commands {
                yaml.push_str(&format!("    - {}\n", cmd.value));
            }
        }
    }

    // Placeholders for unrecoverable sections
    yaml.push_str("\n# [*INCOMPLETE*] Workflows not recoverable from export\n");
    yaml.push_str("# workflows:\n");
    yaml.push_str("#   default:\n");
    yaml.push_str("#     file: ./workflows/main.yaml\n");
    yaml.push_str("#     description: Main workflow\n");
    yaml.push_str("#     default: true\n");

    yaml.push_str("\n# [*INCOMPLETE*] Config options not recoverable from export\n");
    yaml.push_str("# config:\n");
    yaml.push_str("#   option_name:\n");
    yaml.push_str("#     type: string\n");
    yaml.push_str("#     description: Option description\n");
    yaml.push_str("#     default: \"value\"\n");

    yaml.push_str("\n# [*INCOMPLETE*] Auth requirements - verify these\n");
    yaml.push_str("# auth:\n");
    yaml.push_str("#   daemons:\n");
    yaml.push_str("#     daemon_name: required\n");

    yaml
}

/// Generate import report markdown
fn generate_import_report(
    skill: &ImportedSkill,
    enrichment: Option<&EnrichmentData>,
    quality: Option<&QualityAssessment>,
    sync: Option<&SyncAnalysis>,
) -> String {
    let mut report = String::new();

    report.push_str(&format!("# Import Report: {}\n\n", skill.name.value));
    report.push_str(&format!(
        "**Source:** {} ({} format)\n",
        skill.source_path.display(),
        skill.source_format.name()
    ));
    report.push_str(&format!("**Imported:** {}\n", skill.import_timestamp));

    // Quality grade at the top if available
    if let Some(q) = quality {
        report.push_str(&format!(
            "**Quality Grade:** {} {:?} - {} ({}%)\n",
            q.grade.emoji(),
            q.grade,
            q.grade.description(),
            q.score
        ));
    } else {
        report.push_str(&format!(
            "**Overall Confidence:** {}%\n",
            skill.confidence_score()
        ));
    }

    // Sync status
    if let Some(s) = sync {
        report.push_str(&format!(
            "**Sync Status:** {} {}\n",
            s.status.emoji(),
            s.status.description()
        ));
    }
    if enrichment.is_some() {
        report.push_str("**Enriched:** Yes (daemon registry lookup)\n");
    }
    report.push_str("\n");

    // Field recovery summary
    report.push_str("## Field Recovery Summary\n\n");
    report.push_str("| Field | Confidence | Source | Notes |\n");
    report.push_str("|-------|------------|--------|-------|\n");

    let conf_emoji = |c: Confidence| match c {
        Confidence::High => "✅ High",
        Confidence::Medium => "⚠️ Medium",
        Confidence::Low => "❌ Low",
        Confidence::Unknown => "❓ Unknown",
    };

    report.push_str(&format!(
        "| name | {} | {:?} | {} |\n",
        conf_emoji(skill.name.confidence),
        skill.name.source,
        skill.name.notes.as_deref().unwrap_or("-")
    ));
    report.push_str(&format!(
        "| version | {} | {:?} | {} |\n",
        conf_emoji(skill.version.confidence),
        skill.version.source,
        skill.version.notes.as_deref().unwrap_or("-")
    ));
    report.push_str(&format!(
        "| description | {} | {:?} | {} |\n",
        conf_emoji(skill.description.confidence),
        skill.description.source,
        skill.description.notes.as_deref().unwrap_or("-")
    ));
    report.push_str(&format!(
        "| instructions | {} | {:?} | {} |\n",
        conf_emoji(skill.instructions_content.confidence),
        skill.instructions_content.source,
        "-"
    ));

    // Daemons
    if !skill.daemons.is_empty() {
        let daemon_names: Vec<_> = skill.daemons.iter().map(|d| d.name.value.as_str()).collect();
        let method_count: usize = skill.daemons.iter().map(|d| d.methods.len()).sum();
        report.push_str(&format!(
            "| daemons | {} | Extracted | {} daemons, {} methods |\n",
            conf_emoji(Confidence::Medium),
            daemon_names.len(),
            method_count
        ));
    } else {
        report.push_str("| daemons | ❌ None | - | No daemons detected |\n");
    }

    // Triggers
    let trigger_count =
        skill.triggers.keywords.len() + skill.triggers.patterns.len() + skill.triggers.commands.len();
    if trigger_count > 0 {
        report.push_str(&format!(
            "| triggers | {} | Mixed | {} total |\n",
            conf_emoji(Confidence::Medium),
            trigger_count
        ));
    } else {
        report.push_str("| triggers | ❌ None | - | No triggers detected |\n");
    }

    // Auth from enrichment
    if let Some(e) = enrichment {
        if !e.auth_requirements.is_empty() {
            report.push_str(&format!(
                "| auth | ✅ High | Registry | {} daemons require auth |\n",
                e.auth_requirements.len()
            ));
        } else {
            report.push_str("| auth | ⚠️ Medium | Registry | No auth requirements found |\n");
        }
    } else {
        report.push_str("| auth | ❌ None | N/A | Not in export format |\n");
    }

    // Always missing
    report.push_str("| workflows | ❌ None | N/A | Not in export format |\n");
    report.push_str("| config | ❌ None | N/A | Not in export format |\n");

    // Enrichment section
    if let Some(e) = enrichment {
        report.push_str("\n## Registry Enrichment\n\n");

        if !e.verified_daemons.is_empty() {
            report.push_str("### Verified Daemons\n\n");
            for daemon in &e.verified_daemons {
                report.push_str(&format!("- ✅ **{}** - Found in daemon registry\n", daemon));
            }
            report.push_str("\n");
        }

        if !e.unknown_daemons.is_empty() {
            report.push_str("### Unknown Daemons\n\n");
            for daemon in &e.unknown_daemons {
                report.push_str(&format!("- ❓ **{}** - Not found in registry (may be custom)\n", daemon));
            }
            report.push_str("\n");
        }

        if !e.auth_requirements.is_empty() {
            report.push_str("### Authentication Requirements\n\n");
            for (daemon, auth) in &e.auth_requirements {
                let auth_type = auth.auth_type.as_deref().unwrap_or("unknown");
                let provider = auth.provider.as_deref().unwrap_or("N/A");
                report.push_str(&format!(
                    "- **{}**: {} auth via {}\n",
                    daemon, auth_type, provider
                ));
                if !auth.scopes.is_empty() {
                    report.push_str(&format!("  - Scopes: {}\n", auth.scopes.join(", ")));
                }
            }
            report.push_str("\n");
        }

        if !e.method_descriptions.is_empty() {
            report.push_str("### Method Details from Registry\n\n");
            let mut methods: Vec<_> = e.method_descriptions.iter().collect();
            methods.sort_by_key(|(k, _)| k.as_str());
            for (method, desc) in methods {
                report.push_str(&format!("- `{}`: {}\n", method, desc));
            }
            report.push_str("\n");
        }

        if !e.platform_support.is_empty() {
            report.push_str("### Platform Support\n\n");
            for (daemon, platforms) in &e.platform_support {
                report.push_str(&format!("- **{}**: {}\n", daemon, platforms.join(", ")));
            }
            report.push_str("\n");
        }
    }

    // Quality Assessment section (new)
    if let Some(q) = quality {
        report.push_str("\n## Quality Assessment\n\n");

        // Score breakdown
        report.push_str("### Score Breakdown\n\n");
        report.push_str("| Category | Score | Weight | Contribution |\n");
        report.push_str("|----------|-------|--------|-------------|\n");
        report.push_str(&format!(
            "| Metadata | {}% | 25% | {}pts |\n",
            q.breakdown.metadata_score,
            q.breakdown.metadata_score * 25 / 100
        ));
        report.push_str(&format!(
            "| Daemons | {}% | 30% | {}pts |\n",
            q.breakdown.daemon_score,
            q.breakdown.daemon_score * 30 / 100
        ));
        report.push_str(&format!(
            "| Instructions | {}% | 25% | {}pts |\n",
            q.breakdown.instructions_score,
            q.breakdown.instructions_score * 25 / 100
        ));
        report.push_str(&format!(
            "| Triggers | {}% | 10% | {}pts |\n",
            q.breakdown.trigger_score,
            q.breakdown.trigger_score * 10 / 100
        ));
        report.push_str(&format!(
            "| Config/Auth | {}% | 10% | {}pts |\n",
            q.breakdown.config_score,
            q.breakdown.config_score * 10 / 100
        ));
        report.push_str(&format!("| **Total** | | | **{}pts** |\n\n", q.score));

        // Issues by priority
        if !q.issues.is_empty() {
            report.push_str("### Issues Found\n\n");

            let critical: Vec<_> = q.issues.iter().filter(|i| i.priority == Priority::Critical).collect();
            let high: Vec<_> = q.issues.iter().filter(|i| i.priority == Priority::High).collect();
            let medium: Vec<_> = q.issues.iter().filter(|i| i.priority == Priority::Medium).collect();
            let low: Vec<_> = q.issues.iter().filter(|i| i.priority == Priority::Low).collect();

            if !critical.is_empty() {
                report.push_str("#### 🚨 Critical\n\n");
                for issue in critical {
                    report.push_str(&format!("- **{}**: {}\n", issue.field, issue.message));
                    if let Some(ref suggestion) = issue.suggestion {
                        report.push_str(&format!("  - *Fix:* {}\n", suggestion));
                    }
                }
                report.push_str("\n");
            }

            if !high.is_empty() {
                report.push_str("#### ⚠️ High Priority\n\n");
                for issue in high {
                    report.push_str(&format!("- **{}**: {}\n", issue.field, issue.message));
                    if let Some(ref suggestion) = issue.suggestion {
                        report.push_str(&format!("  - *Fix:* {}\n", suggestion));
                    }
                }
                report.push_str("\n");
            }

            if !medium.is_empty() {
                report.push_str("#### 📝 Medium Priority\n\n");
                for issue in medium {
                    report.push_str(&format!("- **{}**: {}\n", issue.field, issue.message));
                    if let Some(ref suggestion) = issue.suggestion {
                        report.push_str(&format!("  - *Fix:* {}\n", suggestion));
                    }
                }
                report.push_str("\n");
            }

            if !low.is_empty() {
                report.push_str("#### 💡 Low Priority\n\n");
                for issue in low {
                    report.push_str(&format!("- **{}**: {}\n", issue.field, issue.message));
                    if let Some(ref suggestion) = issue.suggestion {
                        report.push_str(&format!("  - *Fix:* {}\n", suggestion));
                    }
                }
                report.push_str("\n");
            }
        }

        // Recommendations
        if !q.recommendations.is_empty() {
            report.push_str("### Recommendations\n\n");
            for (i, rec) in q.recommendations.iter().enumerate() {
                report.push_str(&format!(
                    "{}. {} **{}** ({})\n",
                    i + 1,
                    rec.priority.emoji(),
                    rec.title,
                    rec.effort
                ));
                report.push_str(&format!("   - {}\n", rec.description));
                report.push_str(&format!("   - Action: {}\n\n", rec.action));
            }
        }

        // Format limitations
        if !q.format_limitations.is_empty() {
            report.push_str("### Format Limitations\n\n");
            report.push_str(&format!(
                "The {} format has inherent limitations:\n\n",
                skill.source_format.name()
            ));
            for limitation in &q.format_limitations {
                report.push_str(&format!("- {}\n", limitation));
            }
            report.push_str("\n");
        }
    } else {
        // Fallback to old-style required actions if no quality assessment
        report.push_str("\n## Required User Actions\n\n");
        report.push_str("1. **Verify daemon version constraints** - Currently set to `>=1.0.0`\n");
        report.push_str("2. **Add author information** - Name, email, URL\n");
        report.push_str(
            "3. **Add workflow definitions** if this skill has multi-step operations\n",
        );
        report.push_str("4. **Define config options** for user-customizable behavior\n");
        if enrichment.is_none() {
            report.push_str("5. **Review auth requirements** - Run with `--enrich` or check daemons manually\n");
        }
        report.push_str("6. **Verify license** - Default is MIT\n");
    }

    // Sync tracking section
    if let Some(s) = sync {
        report.push_str("\n## Sync Tracking\n\n");

        report.push_str(&format!(
            "**Status:** {} {}\n\n",
            s.status.emoji(),
            s.status.description()
        ));

        // Show diffs if any
        if !s.diffs.is_empty() {
            report.push_str("### Changes Detected\n\n");
            report.push_str("| Field | Change | Significance | Details |\n");
            report.push_str("|-------|--------|--------------|--------|\n");

            for diff in &s.diffs {
                let change_str = match diff.change_type {
                    ChangeType::Added => "+ Added",
                    ChangeType::Removed => "- Removed",
                    ChangeType::Modified => "~ Modified",
                    ChangeType::Unchanged => "= Same",
                };
                let details = match (&diff.original_value, &diff.current_value) {
                    (Some(orig), Some(curr)) => format!("{} → {}", orig, curr),
                    (None, Some(curr)) => format!("→ {}", curr),
                    (Some(orig), None) => format!("{} →", orig),
                    (None, None) => "-".to_string(),
                };
                report.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    diff.field,
                    change_str,
                    diff.significance.emoji(),
                    details
                ));
            }
            report.push_str("\n");
        }

        // Recommendation
        report.push_str("### Recommendation\n\n");
        report.push_str(&format!("{}\n\n", s.recommendation.description));

        if !s.recommendation.commands.is_empty() {
            report.push_str("**Suggested commands:**\n```bash\n");
            for cmd in &s.recommendation.commands {
                report.push_str(&format!("{}\n", cmd));
            }
            report.push_str("```\n\n");
        }

        // Fingerprint info
        report.push_str("### Fingerprint\n\n");
        report.push_str(&format!(
            "- **Hash:** `{:016x}`\n",
            s.current_fingerprint.combined_hash
        ));
        report.push_str(&format!(
            "- **Timestamp:** {}\n",
            s.current_fingerprint.timestamp
        ));
        if let Some(ref prev) = s.previous_fingerprint {
            report.push_str(&format!(
                "- **Previous Hash:** `{:016x}`\n",
                prev.combined_hash
            ));
            report.push_str(&format!(
                "- **Previous Sync:** {}\n",
                prev.timestamp
            ));
        }
    }

    // Unrecoverable data
    report.push_str("\n## Unrecoverable Data\n\n");
    report.push_str(
        "The following data cannot be recovered from exports and must be manually added:\n\n",
    );
    report.push_str("- **Workflow YAML files** - Original workflow logic is not exported\n");
    report.push_str("- **JSON Schema definitions** - Full parameter schemas are simplified\n");
    report.push_str("- **Marketplace/distribution config** - Not relevant to agent exports\n");
    report.push_str("- **Entitlements and licensing** - Commercial terms not exported\n");

    report
}

// ============================================================================
// Public API
// ============================================================================

/// Import a skill from a file
pub fn import_skill(
    path: &str,
    format: Option<&str>,
    output: Option<&str>,
    dry_run: bool,
    enrich: bool,
) -> Result<()> {
    let source_path = Path::new(path);

    if !source_path.exists() {
        bail!("File not found: {}", path);
    }

    // Detect or use specified format
    let import_format = if let Some(fmt) = format {
        match fmt.to_lowercase().as_str() {
            "claude-code" | "claude" => ImportFormat::ClaudeCode,
            "cursor" => ImportFormat::Cursor,
            "codex" => ImportFormat::Codex,
            "mcp" => ImportFormat::Mcp,
            "zed" => ImportFormat::Zed,
            "windsurf" => ImportFormat::Windsurf,
            "gemini" => ImportFormat::Gemini,
            "aider" => ImportFormat::Aider,
            _ => bail!("Unknown format: {}", fmt),
        }
    } else {
        ImportFormat::detect(source_path).ok_or_else(|| {
            anyhow::anyhow!(
                "Could not detect format. Use --format to specify.\n\
                 Valid formats: claude-code, cursor, codex, mcp, zed, windsurf, gemini, aider"
            )
        })?
    };

    println!(
        "{} Importing from {} format...",
        "→".blue().bold(),
        import_format.name().cyan()
    );

    // Read content
    let content = fs::read_to_string(source_path)
        .with_context(|| format!("Failed to read {}", source_path.display()))?;

    // Parse based on format
    let mut skill = match import_format {
        ImportFormat::ClaudeCode => parse_claude_code(source_path, &content)?,
        ImportFormat::Cursor => parse_cursor(source_path, &content)?,
        ImportFormat::Zed => parse_zed(source_path, &content)?,
        ImportFormat::Windsurf => parse_windsurf(source_path, &content)?,
        ImportFormat::Aider => parse_aider(source_path, &content)?,
        ImportFormat::Gemini => parse_gemini(source_path, &content)?,
        ImportFormat::Codex => parse_codex(source_path, &content)?,
        ImportFormat::Mcp => parse_mcp(source_path, &content)?,
    };

    // Optionally enrich with daemon registry data
    let enrichment = if enrich {
        println!(
            "{} Loading daemon registry...",
            "→".blue().bold()
        );
        match DaemonRegistry::load_default() {
            Ok(registry) => {
                if registry.daemon_count() > 0 {
                    println!(
                        "  {} Loaded {} daemons: [{}]",
                        "✓".green(),
                        registry.daemon_count(),
                        registry.daemon_names().join(", ")
                    );
                    let enrichment_data = enrich_skill(&mut skill, &registry);

                    if !enrichment_data.verified_daemons.is_empty() {
                        println!(
                            "  {} Verified daemons: [{}]",
                            "✓".green(),
                            enrichment_data.verified_daemons.join(", ")
                        );
                    }
                    if !enrichment_data.unknown_daemons.is_empty() {
                        println!(
                            "  {} Unknown daemons: [{}]",
                            "?".yellow(),
                            enrichment_data.unknown_daemons.join(", ")
                        );
                    }
                    if !enrichment_data.auth_requirements.is_empty() {
                        println!(
                            "  {} Auth required: [{}]",
                            "!".cyan(),
                            enrichment_data.auth_requirements.keys().cloned().collect::<Vec<_>>().join(", ")
                        );
                    }
                    Some(enrichment_data)
                } else {
                    println!(
                        "  {} No daemon manifests found",
                        "?".yellow()
                    );
                    None
                }
            }
            Err(e) => {
                println!(
                    "  {} Failed to load registry: {}",
                    "✗".red(),
                    e
                );
                None
            }
        }
    } else {
        None
    };

    // Print extraction summary
    println!();
    println!("{}:", "Extracted".bold());
    println!(
        "  {} name: {}",
        skill.name.confidence.symbol().to_string().green(),
        skill.name.value.cyan()
    );
    println!(
        "  {} description: {}",
        skill.description.confidence.symbol(),
        truncate(&skill.description.value, 50)
    );
    println!(
        "  {} version: {}",
        skill.version.confidence.symbol(),
        skill.version.value
    );

    if !skill.daemons.is_empty() {
        let daemon_info: Vec<String> = skill
            .daemons
            .iter()
            .map(|d| format!("{} ({} methods)", d.name.value, d.methods.len()))
            .collect();
        println!(
            "  {} daemons: {}",
            Confidence::Medium.symbol(),
            daemon_info.join(", ")
        );
    } else {
        println!("  {} daemons: none detected", Confidence::Unknown.symbol());
    }

    let trigger_count =
        skill.triggers.keywords.len() + skill.triggers.patterns.len() + skill.triggers.commands.len();
    if trigger_count > 0 {
        let keywords: Vec<_> = skill.triggers.keywords.iter().map(|k| k.value.as_str()).collect();
        println!(
            "  {} triggers: [{}]",
            Confidence::Medium.symbol(),
            keywords.join(", ")
        );
    }

    // Perform quality assessment
    let quality = analyze_quality(&skill, enrichment.as_ref());

    println!();
    println!(
        "Quality Grade: {} {:?} - {} ({}%)",
        quality.grade.emoji(),
        quality.grade,
        quality.grade.description(),
        quality.score.to_string().cyan()
    );

    // Show critical/high issues count
    let critical_count = quality.issues.iter().filter(|i| i.priority == Priority::Critical).count();
    let high_count = quality.issues.iter().filter(|i| i.priority == Priority::High).count();
    if critical_count > 0 || high_count > 0 {
        if critical_count > 0 {
            println!(
                "  {} {} critical issue(s)",
                "🚨".red(),
                critical_count
            );
        }
        if high_count > 0 {
            println!(
                "  {} {} high priority issue(s)",
                "⚠️".yellow(),
                high_count
            );
        }
    }

    if dry_run {
        println!();
        println!("{}", "Dry run - no files written.".yellow());
        println!();
        println!("Would generate:");
        println!("  → skill.yaml");
        println!("  → instructions/core.md");
        println!(
            "  → instructions/{}.md",
            skill.source_format.to_key()
        );
        println!("  → IMPORT_REPORT.md");
        return Ok(());
    }

    // Determine output directory
    let output_dir = match output {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir()?.join(&skill.name.value),
    };

    // Create directory structure
    fs::create_dir_all(&output_dir)?;
    fs::create_dir_all(output_dir.join("instructions"))?;
    fs::create_dir_all(output_dir.join("workflows"))?;

    // Write skill.yaml
    let skill_yaml = generate_skill_yaml(&skill);
    let skill_yaml_path = output_dir.join("skill.yaml");
    fs::write(&skill_yaml_path, &skill_yaml)?;
    println!();
    println!("{} {}", "→".blue(), skill_yaml_path.display());

    // Write instructions/core.md
    let core_md_path = output_dir.join("instructions").join("core.md");
    fs::write(&core_md_path, &skill.instructions_content.value)?;
    println!("{} {}", "→".blue(), core_md_path.display());

    // Write instructions/{agent}.md (copy of original)
    let agent_md_path = output_dir
        .join("instructions")
        .join(format!("{}.md", skill.source_format.to_key()));
    fs::write(&agent_md_path, &content)?;
    println!("{} {}", "→".blue(), agent_md_path.display());

    // Analyze sync status (check if output directory already has a skill)
    let sync_analysis = analyze_sync(&skill, Some(&output_dir));

    // Write import report with quality assessment and sync status
    let report = generate_import_report(&skill, enrichment.as_ref(), Some(&quality), Some(&sync_analysis));
    let report_path = output_dir.join("IMPORT_REPORT.md");
    fs::write(&report_path, &report)?;
    println!("{} {}", "→".blue(), report_path.display());

    // Write sync metadata for future comparisons
    let sync_metadata = generate_sync_metadata(&skill);
    let sync_path = output_dir.join(".sync.json");
    fs::write(&sync_path, &sync_metadata)?;
    println!("{} {} (sync tracking)", "→".blue(), sync_path.display());

    // Add .gitkeep to workflows
    fs::write(output_dir.join("workflows").join(".gitkeep"), "")?;

    println!();
    println!(
        "{} Import complete! Review {} for required actions.",
        "✓".green().bold(),
        "IMPORT_REPORT.md".cyan()
    );

    // Show sync status
    println!(
        "{} Sync: {} {}",
        sync_analysis.status.emoji(),
        match sync_analysis.status {
            SyncStatus::InSync => "In sync".green(),
            SyncStatus::SourceNewer => "Source newer".yellow(),
            SyncStatus::CanonicalNewer => "Canonical newer".yellow(),
            SyncStatus::Diverged => "Diverged".red(),
            SyncStatus::Unknown => "New skill".cyan(),
        },
        format!("(hash: {:016x})", sync_analysis.current_fingerprint.combined_hash).dimmed()
    );

    Ok(())
}

/// Truncate a string to a maximum length
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_yaml_frontmatter() {
        let content = r#"---
name: test
description: A test skill
---

# Test Skill

This is the body.
"#;
        let (fm, body) = extract_yaml_frontmatter(content).unwrap();
        assert!(fm.contains("name: test"));
        assert!(body.contains("# Test Skill"));
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(
            ImportFormat::detect(Path::new("SKILL.md")),
            Some(ImportFormat::ClaudeCode)
        );
        assert_eq!(
            ImportFormat::detect(Path::new(".cursorrules")),
            Some(ImportFormat::Cursor)
        );
        assert_eq!(
            ImportFormat::detect(Path::new("test.mcp.json")),
            Some(ImportFormat::Mcp)
        );
    }

    #[test]
    fn test_extract_first_paragraph() {
        let body = r#"# Header

This is the first paragraph.

This is the second.
"#;
        let para = extract_first_paragraph(body);
        assert_eq!(para, "This is the first paragraph.");
    }
}

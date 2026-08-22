//! Self-contained isometric projection types and pipeline visualization primitives.
//!
//! No external dependencies beyond std. All coordinate values use f32.
//! Projection formula matches JS rhai-live-core.js:
//!   screen_x = origin_x + (pt.x - pt.z) * scale * 0.866
//!   screen_y = origin_y + (pt.x + pt.z) * scale * 0.5 - pt.y * scale

// ============================================================================
// CORE GEOMETRY
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct IsoProjected {
    pub screen_x: f32,
    pub screen_y: f32,
}

/// Project a 3-D point into 2-D screen space using the isometric formula.
#[inline]
pub fn iso_project(pt: Vec3, scale: f32, origin_x: f32, origin_y: f32) -> IsoProjected {
    IsoProjected {
        screen_x: origin_x + (pt.x - pt.z) * scale * 0.866,
        screen_y: origin_y + (pt.x + pt.z) * scale * 0.5 - pt.y * scale,
    }
}

// ============================================================================
// Z-LAYER STACK
// ============================================================================

/// Semantic depth layers for the pipeline visualization stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum ZLayer {
    Document,
    Pipeline,
    Constraint,
    Legal,
    FormalProof,
    Attestation,
    /// SysML-v2 systems-modeling primitives (Requirement/Decision/Cost) —
    /// a dedicated layer rather than folded into the (still-unimplemented,
    /// see `docs/ontological-implementation-spec.md` §6.1) proposed `Domain`
    /// layer, per explicit decision: independent toggle/color in the
    /// isometric renderer over reusing an ontological-concepts layer.
    SystemsModel,
}

impl ZLayer {
    /// 0-based layer index (max 6).
    pub fn index(self) -> u8 {
        match self {
            ZLayer::Document => 0,
            ZLayer::Pipeline => 1,
            ZLayer::Constraint => 2,
            ZLayer::Legal => 3,
            ZLayer::FormalProof => 4,
            ZLayer::Attestation => 5,
            ZLayer::SystemsModel => 6,
        }
    }

    /// Canonical hex color for this layer.
    pub fn color(self) -> &'static str {
        match self {
            ZLayer::Document => "#334155",
            ZLayer::Pipeline => "#1d4ed8",
            ZLayer::Constraint => "#7c3aed",
            ZLayer::Legal => "#b91c1c",
            ZLayer::FormalProof => "#0f766e",
            ZLayer::Attestation => "#b45309",
            ZLayer::SystemsModel => "#be185d",
        }
    }

    /// Base Z offset (world units) for this layer.
    pub fn base_z(self) -> f32 {
        match self {
            ZLayer::Document => 0.0,
            ZLayer::Pipeline => 136.0,
            ZLayer::Constraint => 272.0,
            ZLayer::Legal => 408.0,
            ZLayer::FormalProof => 544.0,
            ZLayer::Attestation => 680.0,
            ZLayer::SystemsModel => 816.0,
        }
    }

    /// Human-readable label for this layer.
    pub fn label(self) -> &'static str {
        match self {
            ZLayer::Document => "Document",
            ZLayer::Pipeline => "Pipeline",
            ZLayer::Constraint => "Constraint",
            ZLayer::Legal => "Legal",
            ZLayer::FormalProof => "FormalProof",
            ZLayer::Attestation => "Attestation",
            ZLayer::SystemsModel => "SystemsModel",
        }
    }
}

impl std::fmt::Display for ZLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

// ============================================================================
// SEMANTIC TYPE
// ============================================================================

/// Classification of a pipeline object's semantic role for visualization routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum SemanticType {
    Document,
    Pipeline,
    Constraint,
    Gate,
    Legal,
    Solver,
    Result,
    Flag,
    Issue,
    Proof,
    Attestation,
    Unknown,
}

impl SemanticType {
    /// Stable lowercase name for DSL / JSON serialization.
    pub fn known_name(self) -> &'static str {
        match self {
            SemanticType::Document => "document",
            SemanticType::Pipeline => "pipeline",
            SemanticType::Constraint => "constraint",
            SemanticType::Gate => "gate",
            SemanticType::Legal => "legal",
            SemanticType::Solver => "solver",
            SemanticType::Result => "result",
            SemanticType::Flag => "flag",
            SemanticType::Issue => "issue",
            SemanticType::Proof => "proof",
            SemanticType::Attestation => "attestation",
            SemanticType::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for SemanticType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.known_name())
    }
}

// ============================================================================
// RHAI DSL TYPE
// ============================================================================

/// Classification of a symbol extracted from a Rhai DSL snippet.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DslSymbolKind {
    FunctionCall,
    Variable,
    Keyword,
}

/// Source position within a DSL snippet (1-based line and column).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DslSpan {
    pub line: u32,
    pub col: u32,
}

/// A named symbol extracted from a Rhai DSL snippet, with kind and source position.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DslSymbol {
    pub kind: DslSymbolKind,
    pub name: String,
    pub span: Option<DslSpan>,
}

/// A Rhai DSL snippet that is both displayable as source text and parseable as an AST.
///
/// Holds a `&'static str` for zero-allocation use in const contexts. For runtime-
/// constructed or deserialized snippets use `RhaiDslOwned`.
#[derive(Debug, Clone, Copy)]
pub struct RhaiDsl {
    source: &'static str,
}

impl RhaiDsl {
    pub const fn new(source: &'static str) -> Self {
        Self { source }
    }

    pub fn source(&self) -> &str {
        self.source
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    /// Parse and validate the snippet via the Rhai engine. Returns the AST on success.
    pub fn parse(&self) -> Result<rhai::AST, rhai::ParseError> {
        rhai::Engine::new().compile(self.source)
    }

    /// Extract named symbols (function calls, variables, keywords) with source positions.
    pub fn symbols(&self) -> Vec<DslSymbol> {
        extract_dsl_symbols(self.source)
    }
}

impl std::fmt::Display for RhaiDsl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.source)
    }
}

impl serde::Serialize for RhaiDsl {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.source)
    }
}

/// Owned, deserializable counterpart to `RhaiDsl` for manifest serialization and
/// runtime-constructed snippets. Shares the same parse/symbol API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RhaiDslOwned {
    pub source: String,
    pub symbols: Vec<DslSymbol>,
}

impl RhaiDslOwned {
    pub fn new(source: impl Into<String>) -> Self {
        let source = source.into();
        let symbols = extract_dsl_symbols(&source);
        Self { source, symbols }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }

    /// Parse and validate the snippet via the Rhai engine.
    pub fn parse(&self) -> Result<rhai::AST, rhai::ParseError> {
        rhai::Engine::new().compile(&self.source)
    }
}

impl std::fmt::Display for RhaiDslOwned {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.source)
    }
}

impl From<RhaiDsl> for RhaiDslOwned {
    fn from(dsl: RhaiDsl) -> Self {
        Self::new(dsl.source)
    }
}

const RHAI_KEYWORDS: &[&str] = &[
    "let", "const", "if", "else", "match", "fn", "for", "while", "loop", "return", "break",
    "continue", "true", "false", "import", "export", "throw", "try", "catch", "in", "is",
];

/// Extract identifiers from Rhai source with line/col spans and kind classification.
/// FunctionCall: identifier immediately followed by `(`.
/// Keyword: one of the Rhai reserved keywords.
/// Variable: any other bare identifier.
fn extract_dsl_symbols(source: &str) -> Vec<DslSymbol> {
    let mut symbols = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    let mut line = 1u32;
    let mut col = 1u32;

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            col = 1;
            i += 1;
            continue;
        }
        // Skip line comments
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // Skip string literals (avoid picking up identifiers inside strings)
        if c == '"' || c == '\'' {
            let quote = c;
            i += 1;
            col += 1;
            while i < chars.len() && chars[i] != quote {
                if chars[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
            i += 1;
            col += 1;
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let span = DslSpan { line, col };
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
                col += 1;
            }
            let name: String = chars[start..i].iter().collect();
            // Peek past whitespace to detect function call
            let mut j = i;
            while j < chars.len() && chars[j] == ' ' {
                j += 1;
            }
            let kind = if RHAI_KEYWORDS.contains(&name.as_str()) {
                DslSymbolKind::Keyword
            } else if j < chars.len() && chars[j] == '(' {
                DslSymbolKind::FunctionCall
            } else {
                DslSymbolKind::Variable
            };
            symbols.push(DslSymbol {
                kind,
                name,
                span: Some(span),
            });
            continue;
        }
        col += 1;
        i += 1;
    }
    symbols
}

// ============================================================================
// VISUALIZATION SPEC + TRAIT
// ============================================================================

/// Full visualization descriptor for a domain type.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VisualizationSpec {
    pub semantic_type: SemanticType,
    pub z_layer: ZLayer,
    pub rhai_dsl: RhaiDsl,
    pub description: &'static str,
}

/// Implemented by every domain type that participates in the isometric pipeline view.
pub trait HasVisualization {
    fn viz_spec() -> VisualizationSpec;
}

// ============================================================================
// ANIMATION
// ============================================================================

/// Easing curve for isometric object transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IsoEasing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Spring,
}

impl IsoEasing {
    /// CSS `transition-timing-function` value.
    pub fn css_value(self) -> &'static str {
        match self {
            IsoEasing::Linear => "linear",
            IsoEasing::EaseIn => "ease-in",
            IsoEasing::EaseOut => "ease-out",
            IsoEasing::EaseInOut => "ease-in-out",
            // Approximated with a cubic-bezier for CSS compat.
            IsoEasing::Spring => "cubic-bezier(0.175, 0.885, 0.32, 1.275)",
        }
    }

    /// SMIL `keySplines` value (single spline segment).
    fn smil_key_spline(self) -> &'static str {
        match self {
            IsoEasing::Linear => "0 0 1 1",
            IsoEasing::EaseIn => "0.42 0 1 1",
            IsoEasing::EaseOut => "0 0 0.58 1",
            IsoEasing::EaseInOut => "0.42 0 0.58 1",
            IsoEasing::Spring => "0.175 0.885 0.32 1.275",
        }
    }
}

/// A single 3-D-to-3-D movement segment.
#[derive(Debug, Clone)]
pub struct IsoTransform {
    pub from: Vec3,
    pub to: Vec3,
    pub duration_ms: u32,
    pub easing: IsoEasing,
}

/// Ordered sequence of transforms describing an object's path through the scene.
#[derive(Debug, Clone)]
pub struct IsoAnimationPath {
    pub label: String,
    pub transforms: Vec<IsoTransform>,
}

impl IsoAnimationPath {
    /// Emit SVG SMIL `<animateTransform>` markup for each segment.
    ///
    /// Returns an empty string when `transforms` is empty.
    /// Each segment projects `from`/`to` into screen space then emits:
    /// ```xml
    /// <animateTransform attributeName="transform" type="translate"
    ///   from="sx sy" to="tx ty" dur="Nms"
    ///   calcMode="spline" keySplines="..." keyTimes="0;1"
    ///   additive="replace" fill="freeze" />
    /// ```
    pub fn to_smil_svg(&self, scale: f32, origin_x: f32, origin_y: f32) -> String {
        if self.transforms.is_empty() {
            return String::new();
        }
        let mut out = String::new();
        let mut begin_ms: u32 = 0;

        for transform in &self.transforms {
            let from_s = iso_project(transform.from, scale, origin_x, origin_y);
            let to_s = iso_project(transform.to, scale, origin_x, origin_y);
            let dur = transform.duration_ms;
            let spline = transform.easing.smil_key_spline();

            out.push_str(&format!(
                "<animateTransform \
                    attributeName=\"transform\" \
                    type=\"translate\" \
                    from=\"{fx:.3} {fy:.3}\" \
                    to=\"{tx:.3} {ty:.3}\" \
                    dur=\"{dur}ms\" \
                    begin=\"{begin}ms\" \
                    calcMode=\"spline\" \
                    keySplines=\"{spline}\" \
                    keyTimes=\"0;1\" \
                    additive=\"replace\" \
                    fill=\"freeze\" />\n",
                fx = from_s.screen_x,
                fy = from_s.screen_y,
                tx = to_s.screen_x,
                ty = to_s.screen_y,
                dur = dur,
                begin = begin_ms,
                spline = spline,
            ));
            begin_ms = begin_ms.saturating_add(dur);
        }
        out
    }

    /// Emit a Python Manim stub for this path.
    ///
    /// Generates `scene.play(obj.animate.move_to(...))` calls sequenced by
    /// cumulative `run_time` derived from `duration_ms`.
    pub fn to_manim_script(&self, label: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("# Manim animation stub for '{}'\n", label));
        out.push_str("from manim import *\n\n");
        out.push_str(&format!(
            "class {}Scene(Scene):\n    def construct(self):\n",
            sanitize_class_name(label)
        ));
        out.push_str(&format!("        obj = Text(\"{}\")\n", label));
        out.push_str("        self.add(obj)\n");

        for (i, transform) in self.transforms.iter().enumerate() {
            let run_time = transform.duration_ms as f32 / 1000.0;
            out.push_str(&format!(
                "        # segment {i}: ({fx:.2},{fy:.2},{fz:.2}) -> ({tx:.2},{ty:.2},{tz:.2})\n",
                i = i,
                fx = transform.from.x,
                fy = transform.from.y,
                fz = transform.from.z,
                tx = transform.to.x,
                ty = transform.to.y,
                tz = transform.to.z,
            ));
            out.push_str(&format!(
                "        self.play(obj.animate.move_to(np.array([{tx:.2}, {ty:.2}, 0])), run_time={rt:.3})\n",
                tx = transform.to.x,
                ty = transform.to.y,
                rt = run_time,
            ));
        }
        out
    }
}

/// Produce a valid Python class name from an arbitrary label string.
fn sanitize_class_name(label: &str) -> String {
    let mut name: String = label
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if name.is_empty() {
        return "AScene".to_string();
    }
    // Ensure it starts with an uppercase letter, not a digit or underscore.
    if name.starts_with(|c: char| c.is_ascii_digit() || c == '_') {
        name.insert(0, 'A');
    } else if let Some(first) = name.chars().next() {
        // Capitalise the first character.  `to_uppercase` may expand to multiple
        // chars (e.g. 'ß' → "SS") so we collect and splice.
        let upper: String = first.to_uppercase().collect();
        name.replace_range(..first.len_utf8(), &upper);
    }
    name
}

/// Minimal XML attribute value escaper for SMIL output.
/// Escapes characters that are unsafe inside a double-quoted XML attribute value.
/// Callers generating `<animateTransform>` markup with dynamic label content should
/// apply this before embedding string values in attribute positions.
pub fn xml_attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

// ============================================================================
// VIZ MANIFEST (for xtask export and docs UI)
// ============================================================================

/// Serializable mirror of `VisualizationSpec` with owned fields.
/// `rhai_dsl` is a `RhaiDslOwned` so the manifest carries pre-extracted symbols
/// for browser-side LSP/syntax-highlighting without re-parsing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizSpecOwned {
    pub semantic_type: String,
    pub z_layer: String,
    pub rhai_dsl: RhaiDslOwned,
    pub description: String,
}

impl From<VisualizationSpec> for VizSpecOwned {
    fn from(spec: VisualizationSpec) -> Self {
        VizSpecOwned {
            semantic_type: spec.semantic_type.known_name().to_string(),
            z_layer: spec.z_layer.label().to_string(),
            rhai_dsl: RhaiDslOwned::from(spec.rhai_dsl),
            description: spec.description.to_string(),
        }
    }
}

/// Single entry in the viz manifest exported to the docs UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizManifestEntry {
    pub type_name: String,
    pub spec: VizSpecOwned,
}

/// Full visualization manifest for the docs UI, exported by xtask.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct VizManifest {
    pub version: String,
    pub objects: Vec<VizManifestEntry>,
}

impl VizManifestEntry {
    /// Convenience constructor from a HasVisualization type.
    pub fn new(type_name: impl Into<String>, spec: VisualizationSpec) -> Self {
        VizManifestEntry {
            type_name: type_name.into(),
            spec: VizSpecOwned::from(spec),
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_project_matches_js_contract() {
        // 192 * 0.866 = 166.272, 192 * 0.5 = 96.0
        let result = iso_project(Vec3::new(192.0, 0.0, 0.0), 1.0, 0.0, 0.0);
        let expected = IsoProjected {
            screen_x: 166.272,
            screen_y: 96.0,
        };
        let eps = 1e-2_f32;
        assert!(
            (result.screen_x - expected.screen_x).abs() < eps,
            "screen_x: got {}, expected {}",
            result.screen_x,
            expected.screen_x
        );
        assert!(
            (result.screen_y - expected.screen_y).abs() < eps,
            "screen_y: got {}, expected {}",
            result.screen_y,
            expected.screen_y
        );
    }

    #[test]
    fn z_layer_index_range() {
        let all = [
            ZLayer::Document,
            ZLayer::Pipeline,
            ZLayer::Constraint,
            ZLayer::Legal,
            ZLayer::FormalProof,
            ZLayer::Attestation,
            ZLayer::SystemsModel,
        ];
        for layer in all {
            assert!(
                layer.index() <= 6,
                "ZLayer::{:?} has index {} > 6",
                layer,
                layer.index()
            );
        }
    }

    #[test]
    fn z_layer_systems_model_is_dedicated_not_pipeline() {
        // Decision 2: Requirement/Decision/Cost content gets its own layer,
        // not folded into an existing one (e.g. Pipeline or a future Domain).
        assert_eq!(ZLayer::SystemsModel.index(), 6);
        assert_eq!(ZLayer::SystemsModel.label(), "SystemsModel");
        assert_ne!(ZLayer::SystemsModel.color(), ZLayer::Pipeline.color());
        assert_eq!(
            ZLayer::SystemsModel.base_z(),
            ZLayer::Attestation.base_z() + 136.0
        );
    }

    #[test]
    fn semantic_type_known_name_nonempty() {
        let all = [
            SemanticType::Document,
            SemanticType::Pipeline,
            SemanticType::Constraint,
            SemanticType::Gate,
            SemanticType::Legal,
            SemanticType::Solver,
            SemanticType::Result,
            SemanticType::Flag,
            SemanticType::Issue,
            SemanticType::Proof,
            SemanticType::Attestation,
            SemanticType::Unknown,
        ];
        for st in all {
            assert!(
                !st.known_name().is_empty(),
                "SemanticType::{:?} returned empty known_name",
                st
            );
        }
    }

    #[test]
    fn iso_easing_css_values_nonempty() {
        let all = [
            IsoEasing::Linear,
            IsoEasing::EaseIn,
            IsoEasing::EaseOut,
            IsoEasing::EaseInOut,
            IsoEasing::Spring,
        ];
        for e in all {
            assert!(!e.css_value().is_empty());
        }
    }

    #[test]
    fn smil_svg_segments_match_transform_count() {
        let path = IsoAnimationPath {
            label: "test".into(),
            transforms: vec![
                IsoTransform {
                    from: Vec3::new(0.0, 0.0, 0.0),
                    to: Vec3::new(100.0, 0.0, 0.0),
                    duration_ms: 300,
                    easing: IsoEasing::EaseOut,
                },
                IsoTransform {
                    from: Vec3::new(100.0, 0.0, 0.0),
                    to: Vec3::new(100.0, 50.0, 0.0),
                    duration_ms: 200,
                    easing: IsoEasing::Linear,
                },
            ],
        };
        let svg = path.to_smil_svg(1.0, 0.0, 0.0);
        assert_eq!(svg.matches("<animateTransform").count(), 2);
    }

    #[test]
    fn manim_script_contains_label() {
        let path = IsoAnimationPath {
            label: "pipeline_stage".into(),
            transforms: vec![IsoTransform {
                from: Vec3::new(0.0, 0.0, 0.0),
                to: Vec3::new(50.0, 0.0, 136.0),
                duration_ms: 400,
                easing: IsoEasing::Spring,
            }],
        };
        let script = path.to_manim_script("pipeline_stage");
        assert!(script.contains("pipeline_stage"));
        assert!(script.contains("move_to"));
    }

    #[test]
    fn smil_svg_empty_transforms_returns_empty_string() {
        let path = IsoAnimationPath {
            label: "empty".into(),
            transforms: vec![],
        };
        assert!(path.to_smil_svg(1.0, 0.0, 0.0).is_empty());
    }

    #[test]
    fn sanitize_class_name_handles_empty_input() {
        assert_eq!(sanitize_class_name(""), "AScene");
    }

    #[test]
    fn sanitize_class_name_capitalizes_first() {
        assert_eq!(sanitize_class_name("pipeline"), "Pipeline");
    }

    #[test]
    fn sanitize_class_name_prefixes_digit_start() {
        assert_eq!(&sanitize_class_name("3stage")[..1], "A");
    }

    #[test]
    fn xml_attr_escape_handles_special_chars() {
        assert_eq!(
            xml_attr_escape(r#"a & "b" <c>"#),
            "a &amp; &quot;b&quot; &lt;c&gt;"
        );
    }

    #[test]
    fn zlayer_display_nonempty() {
        let all = [
            ZLayer::Document,
            ZLayer::Pipeline,
            ZLayer::Constraint,
            ZLayer::Legal,
            ZLayer::FormalProof,
            ZLayer::Attestation,
            ZLayer::SystemsModel,
        ];
        for layer in all {
            assert!(!layer.to_string().is_empty());
        }
    }

    #[test]
    fn semantic_type_display_nonempty() {
        let all = [
            SemanticType::Document,
            SemanticType::Pipeline,
            SemanticType::Constraint,
            SemanticType::Gate,
            SemanticType::Legal,
            SemanticType::Solver,
            SemanticType::Result,
            SemanticType::Flag,
            SemanticType::Issue,
            SemanticType::Proof,
            SemanticType::Attestation,
            SemanticType::Unknown,
        ];
        for st in all {
            assert!(!st.to_string().is_empty());
        }
    }
}

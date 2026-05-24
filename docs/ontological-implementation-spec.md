# Ontological Implementation Specification: OWL2 vs SysMLv2 Semantics

## Executive Summary

This specification defines a hybrid ontological approach for tax-ledger holon-viz system that leverages OWL2 open-world semantics for extensible domain modeling while maintaining SysMLv2 closed-world semantics for runtime pipeline validation. The design enables true extensible stereotypes through KerML 2.0 profile metamodel rather than UML-style tag-based stereotypes.

## 1. Semantic Foundation: OWL2 vs SysMLv2

### 1.1 Open World Assumption (OWL2)
- **Unknown ≠ False**: Unproven statements remain unknown, not false
- **Distributed Evolution**: Knowledge assumed incomplete, can be extended
- **Semantic Web Native**: Optimized for distributed, evolving information
- **Ontological Extensibility**: New concepts can be added without breaking existing reasoning

**Relevance**: Domain model layer (tax categories, jurisdictions, constraint definitions) benefits from OWL2's ability to handle incomplete knowledge and evolve as tax regulations change.

### 1.2 Closed World Assumption (SysMLv2)
- **Known Only**: Statements true only if explicitly known
- **Completeness Implied**: Model contains all relevant information
- **Systems Engineering Native**: Suited for well-defined system specifications
- **Rigorous Validation**: Deterministic behavior for state machine transitions

**Relevance**: Pipeline execution layer (state machines, commit gates, invariant checking) requires deterministic validation where unknown = failure.

## 2. KerML 2.0 Extensible Stereotypes: Architectural Shift

### 2.1 Historical Context
- **SysML v1**: UML profiles + tag-based stereotypes (limited extensibility)
- **KerML/SysML v2**: Metadata + model libraries (formal semantic foundation)

**Key Finding**: OMG SysML v2 specification replaces v1's stereotype mechanism with KerML's metamodel-based extensibility. [Source: SysML v2 Summit presentation](https://www.omg.org/cgi-bin/doc?syseng/25-03-07.pptx)

### 2.2 KerML Extensibility Pattern
```
KerML Profile:
├─ Metamodel Layer (types, relationships)
├─ Domain Layer (instantiated concepts)
└─ Semantic Layer (metadata, annotations, formal constraints)

Extensibility:
├─ Type Extension: subtype relationships (is-a)
├─ Feature Extension: new attributes/operations
├─ Constraint Extension: OCL/Alloy formal constraints
└─ Semantic Extension: metadata annotations + formal proofs
```

### 2.3 Current holon-viz Implementation Gap
**Status**: Minimal emitters producing basic text representations
- `Owl2Emitter`: OWL2/Turtle fragments (owl:Class, rdfs:subClassOf)
- `SysmlV2Emitter`: SysML v2 block definitions (package, block def, part def)

**Gap Analysis**:
1. No extensible stereotype implementation
2. KerML domain.kerm not leveraged for ontological code generation
3. OWL2 uses naive `rdfs:subClassOf` (lacks domain semantics)
4. SysML v2 lacks profile/metamodel structure

## 3. Proposed Architecture: Hybrid OWL2-SysMLv2 Ontology

### 3.1 Layered Semantic Model

```
┌─────────────────────────────────────────────────────────┐
│ LAYER 0: OWL2 Domain Ontology (Open World)               │
│ ├─ TaxCategory, Jurisdiction, ConstraintDefinition      │
│ ├─ extensible via subclassing (owl:Class hierarchy)    │
│ ├─ formal constraints (owl:equivalentClass, restrictions)│
│ └─ semantic reasoning (Pellet/HermiT)                   │
└─────────────────────────────────────────────────────────┘
                         ↓ owl:imports
┌─────────────────────────────────────────────────────────┐
│ LAYER 1: KerML Profile Metamodel                        │
│ ├─ ZLayer, SemanticType, RhaiDsl (current domain.kerm) │
│ ├─ type relationships (implements, contains, produces) │
│ ├─ stereotype-like metadata via KerML annotations       │
│ └─ formal semantics via KerML's OCL/Alloy integration   │
└─────────────────────────────────────────────────────────┘
                         ↓ kerml:instantiates
┌─────────────────────────────────────────────────────────┐
│ LAYER 2: SysML v2 Process Model (Closed World)          │
│ ├─ PipelineState<Ingested|Validated|...> (state machine)│
│ ├─ CommitGate, LegalSolver (validation components)      │
│ ├─ deterministic transitions (advances_to, validated_by)│
│ └─ Z3/Kasuari formal proof integration                  │
└─────────────────────────────────────────────────────────┘
                         ↓ sysml:realizes
┌─────────────────────────────────────────────────────────┐
│ LAYER 3: Runtime Execution (Tax-Ledger Domain)          │
│ ├─ Rust implementation (ledger-core)                    │
│ ├─ Rhai DSL evaluation (runtime validation)            │
│ ├─ Evidence graph (arc-kit-au)                          │
│ └─ 2D/3D isometric visualization (holon-viz)            │
└─────────────────────────────────────────────────────────┘
```

### 3.2 Stereotype Implementation via KerML Annotations

**Pattern: KerML Annotation as Extensible Stereotype**

```kerm
# base stereotype: HasVisualization (existing domain.kerm)
[[type]]
id = "iso::HasVisualization"
label = "HasVisualization"
kind = "abstract_trait"
# @stereotype: base visualization contract
# @open_world: false (closed world - viz spec required)

# extensible stereotype: TaxableTransaction
[[type]]
id = "tax::TaxableTransaction"
label = "TaxableTransaction"
kind = "domain_concept"
# @stereotype: extends Transaction with tax semantics
# @stereotype_base: "iso::Transaction"
# @stereotype_properties:
#   - tax_category: TaxCategory (required)
#   - jurisdiction: Jurisdiction (required)
#   - gst_rate: Decimal (optional)
# @stereotype_constraints:
#   - "self.amount > 0" (via OCL)
#   - "self.tax_category.jurisdiction == self.jurisdiction"
# @open_world: true (domain extension allowed)

# extensible stereotype: GSTFreeSupply
[[type]]
id = "tax::GSTFreeSupply"
label = "GSTFreeSupply"
kind = "domain_concept"
# @stereotype: specializes TaxableTransaction for GST-free goods
# @stereotype_base: "tax::TaxableTransaction"
# @stereotype_properties:
#   - supply_type: SupplyType (GST_FREE | INPUT_TAXED | OUTPUT_TAXED)
#   - reason_code: String (optional)
# @stereotype_axioms: # OWL2-style open world axioms
#   - "TaxableTransaction and (supply_type value GST_FREE)"
#   - "disjointWith InputTaxedSupply"
# @open_world: true
```

**Key Insight**: KerML annotations (`@stereotype_*`) enable true extensibility:
1. Type-based specialization (not tag-based)
2. Formal constraints via OCL/Alloy integration
3. Open/closed world semantics per type
4. OWL2 axiom emission for domain reasoning

## 4. Runtime Validation: Type Invariant Checking

### 4.1 Declarative Pipeline Workflow

**Pattern: Rhai DSL as Runtime Constraint Evaluator**

```rust
// Runtime validation pipeline
pub trait PipelineInvariant<T> {
    fn invariant_check(&self) -> Result<(), Violation>;

    fn viz_spec() -> VisualizationSpec {
        VisualizationSpec {
            // @stereotype: PipelineInvariant
            // @stereotype_base: "iso::HasVisualization"
            // @stereotype_constraint: "self.confidence >= MIN_CONF"
            semantic_type: SemanticType::Gate,
            z_layer: ZLayer::Pipeline,
            rhai_dsl: RhaiDsl::new(r#"
                // @runtime: type invariant checker
                // @formal: Z3 predicate "amount > 0 && confidence >= 0.8"
                let min_conf = 0.8;
                if tx.amount <= 0 {
                    fail("AMT_NEG", "amount must be positive");
                }
                if meta.confidence < min_conf {
                    fail("LOW_CONF", format!("below threshold {}", min_conf));
                }
                if !jurisdiction.check(tx.tax_category) {
                    fail("JUR_MISMATCH", "category not valid for jurisdiction");
                }
                ok();
            "#),
        },
    }
}

impl PipelineInvariant<Transaction> for ClassifiedState {
    fn invariant_check(&self) -> Result<(), Violation> {
        // Evaluate Rhai DSL at runtime
        let engine = RhaiEngine::new();
        engine.eval(self.viz_spec().rhai_dsl.code())
    }
}
```

### 4.2 Formal Proof Integration

**Z3 Invariant Verification** (existing `legal::Z3Result` pattern):
```rust
// Formal proof predicate (KerML stereotype annotation)
// @stereotype_axiom: "Forall t: Transaction |
//    t.classified_as.category.in_jurisdiction(t.jurisdiction)"
let z3_predicate = r#"
    (declare-const tx Transaction)
    (declare-const category TaxCategory)
    (declare-const jurisdiction Jurisdiction)
    (assert (=> (classified_as tx category)
               (valid_in category jurisdiction)))
    (check-sat)
"#;

// Runtime verification
let result = legal_solver.verify(z3_predicate, facts);
assert_eq!(result, Z3Result::Satisfied);
```

**Kasuari Layout Verification** (existing `pipeline::KasuariSolver`):
```rust
// Layout constraints (Kasuari description)
// @stereotype_layout: "amount ∈ [min, max] ∩ (confidence >= 0.8)"
let kasuari_desc = "amount between 100.00 and 50000.00 with confidence >= 0.8";
let score = kasuari_solver.evaluate("amount", tx.amount, [100.0, 50000.0]);
assert!(score.strength == Strength::Required);
```

### 4.3 State Machine Invariant Enforcement

**Closed-World Validation** (SysML v2 state machine):
```rust
// State transition validation
impl PipelineStateMachine {
    pub fn transition(&self, from: State, to: State) -> Result<(), TransitionError> {
        // @stereotype: StateMachine
        // @stereotype_constraint: "valid_transitions.contains((from, to))"
        match (from, to) {
            (State::Ingested, State::Validated) => {
                // Check invariants before transition
                self.validate_ingestion()?;
                Ok(())
            },
            (State::Validated, State::Classified) => {
                self.validate_classification()?;
                Ok(())
            },
            _ => Err(TransitionError::InvalidTransition(from, to)),
        }
    }
}
```

## 5. Ontological Code Generation Specification

### 5.1 OWL2 Ontology Emitter (Enhanced)

**Current**: `rdfs:subClassOf` naive containment

**Enhanced**:
```turtle
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix tax: <urn:tax-ledger:> .

# Domain ontology (open world)
tax:TaxableTransaction a owl:Class ;
    rdfs:subClassOf tax:Transaction ;
    rdfs:comment "Extensible transaction with tax semantics" ;
    owl:equivalentClass [
        a owl:Restriction ;
        owl:onProperty tax:hasTaxCategory ;
        owl:someValuesFrom tax:TaxCategory
    ] , [
        a owl:Restriction ;
        owl:onProperty tax:hasJurisdiction ;
        owl:someValuesFrom tax:Jurisdiction
    ] .

tax:GSTFreeSupply a owl:Class ;
    rdfs:subClassOf tax:TaxableTransaction ;
    owl:disjointWith tax:InputTaxedSupply ;
    owl:equivalentClass [
        a owl:Restriction ;
        owl:onProperty tax:hasSupplyType ;
        owl:hasValue tax:GST_FREE
    ] .

# Object property constraints
tax:hasTaxCategory a owl:ObjectProperty ;
    rdfs:domain tax:TaxableTransaction ;
    rdfs:range tax:TaxCategory ;
    owl:inverseOf tax:categorizes .

# Data property constraints
tax:gstRate a owl:DatatypeProperty ;
    rdfs:domain tax:TaxableTransaction ;
    rdfs:range xsd:decimal ;
    rdfs:comment "GST rate percentage (0.0-15.0)" .
```

### 5.2 SysML v2 Profile Emitter (New)

**Structure** (KerML-based):
```sysml
package TaxLedgerProfile {
    // KerML metamodel layer
    abstract trait def HasVisualization {
        attribute semantic_type : SemanticType;
        attribute z_layer : ZLayer;
        attribute rhai_dsl : RhaiDsl;
    }

    // Domain layer with stereotypes
    abstract block def TaxableTransaction specializes Transaction {
        // @stereotype: TaxableTransaction
        attribute tax_category : TaxCategory[1];
        attribute jurisdiction : Jurisdiction[1];
        attribute gst_rate : Decimal[0..1];

        // Formal constraints (OCL)
        invariant TaxableConstraint {
            self.amount > 0
            and self.tax_category.jurisdiction == self.jurisdiction
        }
    }

    block def GSTFreeSupply specializes TaxableTransaction {
        attribute supply_type : SupplyType;
        attribute reason_code : String[0..1];

        // OWL2-style axiom translation
        invariant GSTFreeConstraint {
            self.supply_type == SupplyType::GST_FREE
        }
    }

    // State machine (closed world)
    state def PipelineState {
        state Ingested;
        state Validated;
        state Classified;
        state Reconciled;
        state Committed;
        state NeedsReview;

        transition Ingested -> Validated;
        transition Validated -> Classified;
        transition Classified -> Reconciled;
        transition Reconciled -> CommitGate;
        transition CommitGate -> Committed;
        transition CommitGate -> NeedsReview;
    }

    // Validation gate
    block def CommitGate {
        decision approve {
            in StageResult;
            out Committed;
            guard gate.approved;
        }

        decision review {
            in StageResult;
            out NeedsReview;
            guard !gate.approved;
        }
    }
}
```

## 6. 2D/3D Isometric Visualization Requirements

### 6.1 Semantic Differentiation by Z-Layer

**Current**: 6 Z-Layer types (Document, Pipeline, Constraint, Legal, FormalProof, Attestation)

**Enhanced**:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ZLayer {
    Document,       // z=0 (evidence nodes: SourceDoc, ExtractedRow)
    Pipeline,       // z=1 (state machines: PipelineState<*>)
    Constraint,     // z=2 (validation: CommitGate, Issue)
    Legal,          // z=3 (formal verification: Z3Result, Jurisdiction)
    FormalProof,    // z=4 (proof solvers: KasuariSolver, AttestationSpec)
    Attestation,    // z=5 (attestation artifacts: OperatorApproval)
    Domain,         // z=6 (ontological concepts: TaxCategory, Jurisdiction)
    Meta,           // z=7 (profile metamodel: HasVisualization, ZLayer)
}

// 2D isometric projection
impl IsometricProjection for ZLayer {
    fn offset(&self) -> (f64, f64) {
        match self {
            Document => (0.0, 0.0),     // ground layer
            Pipeline => (0.0, 1.5),     // stacked above evidence
            Constraint => (0.0, 3.0),   // validation layer
            Legal => (0.0, 4.5),        // formal verification
            FormalProof => (0.0, 6.0),  // proof layer
            Attestation => (0.0, 7.5),  // attestation layer
            Domain => (0.0, 9.0),       // ontological concepts
            Meta => (0.0, 10.5),        // metamodel layer
        }
    }

    fn color(&self) -> Color {
        match self {
            Document => Color::RGB(0x2E86AB),    // blue (evidence)
            Pipeline => Color::RGB(0xA23B72),    // magenta (execution)
            Constraint => Color::RGB(0xF18F01),  // orange (validation)
            Legal => Color::RGB(0xC73E1D),       // red (formal)
            FormalProof => Color::RGB(0x3B1F2B), // dark purple (proof)
            Attestation => Color::RGB(0x06D6A0), // green (attestation)
            Domain => Color::RGB(0x118AB2),      // cyan (ontology)
            Meta => Color::RGB(0x073B4C),        // navy (metamodel)
        }
    }
}

// 3D volumetric rendering
impl VolumetricRender for TypeNode {
    fn volume(&self) -> BoundingBox {
        let (x, y, z) = match self.z_layer {
            Some(ZLayer::Document) => (2.0, 1.0, 0.5),
            Some(ZLayer::Pipeline) => (3.0, 2.0, 1.0),
            Some(ZLayer::Constraint) => (2.5, 1.5, 0.75),
            Some(ZLayer::Legal) => (4.0, 3.0, 1.5),
            Some(ZLayer::FormalProof) => (3.5, 2.5, 1.25),
            Some(ZLayer::Attestation) => (2.0, 2.0, 1.0),
            Some(ZLayer::Domain) => (5.0, 4.0, 2.0),
            Some(ZLayer::Meta) => (6.0, 5.0, 2.5),
            None => (1.0, 1.0, 1.0),
        };
        BoundingBox::new(x, y, z)
    }
}
```

### 6.2 Relationship Visualization

**Edge Types with Semantic Coloring**:
```rust
impl EdgeVisualization for TypeRelationshipKind {
    fn style(&self) -> EdgeStyle {
        match self {
            // Implementation relationships (dashed, gray)
            Implements => EdgeStyle::dashed(Color::GRAY),

            // Containment (solid, blue)
            Contains => EdgeStyle::solid(Color::BLUE),

            // State transitions (curved, green)
            AdvancesTo => EdgeStyle::curved(Color::GREEN),

            // Constraint relationships (red)
            Constrains => EdgeStyle::dotted(Color::RED),
            Verifies => EdgeStyle::double(Color::RED),

            // Data flow (orange)
            Produces => EdgeStyle::solid(Color::ORANGE),
            ProjectsTo => EdgeStyle::dashed(Color::ORANGE),

            // Classification (purple)
            ClassifiedAs => EdgeStyle::solid(Color::PURPLE),

            // Validation (yellow)
            ValidatedBy => EdgeStyle::thick(Color::YELLOW),

            // Attestation (teal)
            Attests => EdgeStyle::triple(Color::TEAL),

            // Record keeping (brown)
            RecordsIn => EdgeStyle::solid(Color::BROWN),

            // Belongs to (pink)
            BelongsTo => EdgeStyle::dotted(Color::PINK),
        }
    }
}
```

## 7. Rhai DSL Integration Improvements

### 7.1 Current Limitations
1. String-based DSL (no type safety)
2. No static analysis/validation
3. Limited error reporting
4. No formal verification integration

### 7.2 Proposed Enhancements

**Type-Safe DSL Interface**:
```rust
// Rhai AST validation
pub struct RhaiValidator {
    engine: rhai::Engine,
    type_context: TypeContext,
}

impl RhaiValidator {
    pub fn validate(&self, dsl: &RhaiDsl) -> Result<(), ValidationError> {
        // Parse DSL to AST
        let ast = self.engine.compile(dsl.code())?;

        // Type checking
        self.type_context.check_types(&ast)?;

        // Safety checks (no unsafe operations)
        self.check_safety(&ast)?;

        // Formal constraint extraction
        let constraints = self.extract_constraints(&ast)?;
        self.verify_constraints(constraints)?;

        Ok(())
    }

    fn extract_constraints(&self, ast: &AST) -> Result<Vec<FormalConstraint>> {
        // Extract Z3 predicates, Kasuari descriptions
        // Build formal constraint set
    }
}

// Type context for domain types
pub struct TypeContext {
    types: BTreeMap<String, TypeInfo>,
}

impl TypeContext {
    pub fn check_types(&self, ast: &AST) -> Result<(), TypeError> {
        // Check type annotations, function signatures
        // Ensure Rhai types match Rust domain types
    }
}
```

**Runtime Validation Framework**:
```rust
pub trait RuntimeValidator<T> {
    fn validate(&self, instance: &T) -> Result<(), Violation>;

    fn get_invariant(&self) -> FormalInvariant;
}

impl RuntimeValidator<Transaction> for ClassifiedState {
    fn validate(&self, tx: &Transaction) -> Result<(), Violation> {
        // Evaluate Rhai DSL
        let validator = RhaiValidator::new();
        validator.validate(self.viz_spec().rhai_dsl())?;

        // Check invariants
        let invariant = self.get_invariant();
        invariant.check(tx)?;

        Ok(())
    }
}
```

## 8. Implementation Roadmap

### Phase 1: OWL2 Ontology Enhancement (Week 1-2)
1. Enhance `Owl2Emitter` with:
   - Proper OWL2 class hierarchy (not just rdfs:subClassOf)
   - Object/d datatype property constraints
   - EquivalentClass, DisjointClasses axioms
2. Integrate Pellet/HermiT reasoner for validation
3. Add ontological consistency tests

### Phase 2: KerML Profile Implementation (Week 3-4)
1. Implement KerML annotation parser (parse `@stereotype_*` comments)
2. Generate SysML v2 profile from domain.kerm
3. Implement stereotype extension mechanism
4. Add formal constraint integration (OCL/Alloy)

### Phase 3: Runtime Validation (Week 5-6)
1. Enhance Rhai DSL with:
   - Type-safe interface
   - AST validation
   - Formal constraint extraction
2. Implement invariant checking framework
3. Integrate Z3/Kasuari verification
4. Add state machine transition validation

### Phase 4: Isometric Visualization (Week 7-8)
1. Implement 2D isometric projection by Z-Layer
2. Add 3D volumetric rendering support
3. Implement relationship visualization styles
4. Add interactive filtering by semantic layer

### Phase 5: Testing & Validation (Week 9-10)
1. Unit tests for ontological consistency
2. Integration tests for pipeline validation
3. Formal proof verification tests
4. User acceptance testing (CDP demo)

## 9. References

### OMG Standards
- [OMG SysML v2 Specification](https://www.omg.org/spec/SysML/)
- [KerML 2.0 Specification](https://www.omg.org/spec/KerML/)
- [SysML v2 Summit: Stereotypes → Metadata](https://www.omg.org/cgi-bin/doc?syseng/25-03-07.pptx)

### OWL2 & Semantic Web
- [OWL2 Web Ontology Language](https://www.w3.org/TR/owl2-overview/)
- [Open World Assumption vs Closed World](https://indico.esa.int/event/386/contributions/6223/attachments/4266/6464/1015%2520-%2520Q&A.pdf)
- [RDF/OWL2-SysMLv2 Compatibility](https://www.engr.colostate.edu/~drherber/files/Rudder2026a.pdf)

### Ontology Implementations
- [openCAESAR SysML v2 Ontology](https://www.opencaesar.io/events/onto-Nexus-Forum-2025/Talk-11)
- [Modelware SysML v2 Ontology](https://www.modelware.io/raise/SysML-v2-Ontology)

### Formal Verification
- [Z3 SMT Solver](https://github.com/Z3Prover/z3)
- [Kasuari Layout Solver](https://github.com/ianjkoen/kasuari)

### Rust Ecosystem
- [Rhai Scripting Engine](https://rhai.rs/)
- [rust_decimal (Money Type)](https://docs.rs/rust_decimal/)
- [blake3 (Content Hashing)](https://docs.rs/blake3/)

## 10. Conclusion

This specification defines a hybrid ontological approach that:
1. **Leverages OWL2 open-world semantics** for extensible domain modeling
2. **Maintains SysMLv2 closed-world semantics** for deterministic pipeline validation
3. **Implements extensible stereotypes via KerML 2.0 annotations** (not UML tags)
4. **Enables declarative runtime validation** through type invariant checking
5. **Supports 2D/3D isometric visualization** with semantic layer differentiation
6. **Integrates formal verification** (Z3, Kasuari) with runtime evaluation

The architecture balances ontological flexibility (tax domain evolution) with operational rigor (pipeline determinism), enabling tax-ledger to serve as both a semantic reasoning platform and a production-grade financial document intelligence system.

---

**Sources:**
- [How has compatibility with RDF/OWL benefited SysMLv2?](https://indico.esa.int/event/386/contributions/6223/attachments/4266/6464/1015%2520-%2520Q&A.pdf)
- [SysML v2 Summit Presentation](https://www.omg.org/cgi-bin/doc?syseng/25-03-07.pptx)
- [Using an Ontology and Metamodels](https://www.engr.colostate.edu/~drherber/files/Rudder2026a.pdf)
- [Implementing SysML v2 Ontology](https://www.opencaesar.io/events/onto-Nexus-Forum-2025/Talk-11)
- [SysML v2 Ontology](https://www.modelware.io/raise/SysML-v2-Ontology)
- [Uncertainty Modeling for SysML v2](https://arxiv.org/html/2602.21641v1)
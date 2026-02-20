# Obfuscation Research Platform — Final Vision (v3)

---

## Platform Structure

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              SIDEBAR TABS                                    │
│                                                                              │
│  [Playground]  [Experiments]  [Databases]                                    │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│  PLAYGROUND PAGE                                                             │
│                                                                              │
│  ┌────────────────────────────────────────────┬─────────────────────────┐   │
│  │                                            │   RIGHT PANEL           │   │
│  │            CIRCUIT CANVAS                  │   ┌─────────────────┐   │   │
│  │                                            │   │ [Skeleton]      │   │   │
│  │     64 wires × 60,000 gates               │   │ [Database] ←FIX │   │   │
│  │                                            │   │ [Analysis]      │   │   │
│  │     (Scale-adaptive view)                  │   └─────────────────┘   │   │
│  │                                            │                         │   │
│  │                                            │   Skeleton Graph or     │   │
│  │                                            │   DB Search or          │   │
│  │                                            │   Analysis metrics      │   │
│  └────────────────────────────────────────────┴─────────────────────────┘   │
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────────┐ │
│  │  TOOL PANEL (Left): Add gates, Transform, Generate skeleton, etc.     │ │
│  └────────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Part 1: Branch Integration (`many_thread`)

### Status: ✅ COMPLETE (2026-01-22)

See [RAC_INTEGRATION_COMPLETE.md](RAC_INTEGRATION_COMPLETE.md) for full details.

Key functions ported:

| Function | Status | Notes |
|----------|--------|-------|
| `simple_find_convex_subcircuit` | ✅ Done | Working in production |
| `targeted_convex_subcircuit` | ✅ Done | Ported, marked experimental |
| `main_rac_big` | ✅ Done | CLI command `rac` available |
| `replace_and_compress_big` | ✅ Done | Integrated |
| `sequential_compress_big` | ✅ Done | Integrated |
| `replace_sequential_pairs` | ✅ Done | ~260 lines ported |
| `shoot_left_vec` | ✅ Done | Helper ported |

**~1,395 lines of code ported and tested.**

---

## Part 2: Skeleton Chain Synthesis

**Location: `sat_revsynth`** (unrolling is here, NOT local_mixing)

### Architecture

```
sat_revsynth/
├── skeleton synthesis    → Generate constrained chains
├── limited unroll        → Controlled expansion with limits
└── SAT solve remainder   → Complete to identity
```

### API Design

```python
# sat_revsynth/src/skeleton_synthesizer.py

def synthesize_skeleton_chain(
    num_wires: int,
    chain_length: int,
    collision_type: str = "non_commuting"
) -> Circuit:
    """Generate a skeleton chain with specified collision pattern."""
    pass

def limited_unroll(
    skeleton: Circuit,
    depth_limit: int = 10,
    gate_limit: int = 1000
) -> Circuit:
    """Unroll skeleton with bounded expansion to prevent memory blowup."""
    pass

def exhaust_generate(
    skeleton: Circuit,
    must_be_identity: bool = True
) -> Optional[Circuit]:
    """SAT-solve the remainder after skeleton unrolling."""
    pass
```

---

## Part 3: Extreme Scale Visualization (64w / 60k gates)

### WebGL vs react-window

| Aspect | **WebGL** | **react-window** |
|--------|-----------|------------------|
| **What it is** | GPU-accelerated 2D/3D rendering | Virtualized list/grid rendering |
| **Best for** | Rendering thousands of shapes at once | Scrolling through long lists |
| **Gate rendering** | ✅ Excellent: 50k+ shapes at 60fps | ⚠️ Limited: ~1k rows visible |
| **Custom styling** | ⚠️ Must implement all styling in shaders | ✅ Full CSS support |
| **Interactivity** | ⚠️ Custom hit detection needed | ✅ Native click handlers |
| **Learning curve** | High (shaders, buffers) | Low (React component) |
| **Use when** | Need to *render* all gates | Need to *scroll* through gates |

### Recommendation: Hybrid Approach

```
┌─────────────────────────────────────────────────────────────────┐
│                    SCALE-ADAPTIVE STRATEGY                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  SMALL (≤200 gates):                                             │
│    Current Canvas implementation                                 │
│    Full interactivity, drag-drop, editing                        │
│                                                                  │
│  MEDIUM (200-2000 gates):                                        │
│    react-window virtualized view                                 │
│    Scroll through gates, click to select                         │
│    Only render visible rows                                      │
│                                                                  │
│  LARGE (2000+ gates):                                            │
│    Statistics-only dashboard                                     │
│    + Region inspector (show 200-gate windows)                    │
│    + Sample skeleton on selected subcircuit                      │
│                                                                  │
│  EXTREME (60k gates):                                            │
│    Cannot render gates individually                              │
│    Show: wire activity heatmap, gate density graph               │
│    Allow: "Jump to position" → load that region                  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Why Not WebGL for Everything?

1. **Lose CSS styling** — Current theme system wouldn't work
2. **Complex interactivity** — Hover, click, drag need custom implementation
3. **Overkill for medium scale** — react-window handles 2k items easily
4. **High dev effort** — Would need to rebuild entire visualization

### When to Use WebGL

- If you need a **minimap overview** of all 60k gates at once
- For **animation** of gate operations (not just display)
- If react-window virtualization proves too slow (unlikely)

---

## Part 4: Fix Database Panel in Playground

### Current State (RightPanel.tsx)

This has been implemented. The Database tab now calls the unified search endpoint
and supports loading a result onto the canvas.

Key pieces:
- UI: `identity-factory-ui/src/components/playground-v2/RightPanel.tsx`
- API: `/api/v1/search/circuits` (unified search endpoint)

If this regresses, re-check the UI search wiring and the API endpoint shape.

---

## Priority Matrix (Updated 2026-02-03)

| Priority | Task | Track | Effort | Status |
|----------|------|-------|--------|--------|
| ~~🔴 **P0**~~ | ~~Port `simple_find_convex_subcircuit`~~ | Rust | ⚡ Quick | ✅ Done |
| ~~🔴 **P0**~~ | ~~Port RAC mixing scheme~~ | Rust | 🔧 Medium | ✅ Done |
| 🔴 **P0** | Fix Database panel search (RightPanel.tsx) | UI | 🔧 Medium | 🚧 Open |
| 🔴 **P0** | Skeleton chain + limited unroll | Python | 🏗️ Large | ✅ Done (sat_revsynth) |
| 🟡 **P1** | Scale detection in Playground | UI | ⚡ Quick | 🚧 Open |
| 🟡 **P1** | Statistics view for 2k+ gates | UI | 🔧 Medium | 🚧 Open |
| 🟡 **P1** | Region browser (react-window) | UI | 🏗️ Large | 🚧 Open |
| 🟢 **P2** | Test `targeted_convex_subcircuit` | Rust | 🔧 Medium | 🚧 Open |
| 🟢 **P2** | Config schema (after RAC) | Full stack | 🔧 Medium | 🚧 Open |
| 🟢 **P2** | Deploy to GCP | DevOps | 🔧 Medium | 🚧 Open |

---

## Implementation Plans

### Plan 1: Port RAC from many_thread ✅ COMPLETE
See [RAC_INTEGRATION_COMPLETE.md](RAC_INTEGRATION_COMPLETE.md)

### Plan 2: Fix Database Panel 🚧 IN PROGRESS
- Create search API endpoint
- Connect handleSearch to real backend
- Implement "Load to Canvas" action
- Add circuit preview

### Plan 3: Skeleton Chain Synthesis ✅ COMPLETE
See [sat_revsynth/SKELETON_CHAIN_IMPLEMENTATION.md](../../sat_revsynth/SKELETON_CHAIN_IMPLEMENTATION.md)

### Plan 4: Scale-Adaptive Playground 🚧 PLANNED
- Add gate count detection
- Create `StatisticsView` component
- Create `RegionBrowser` with react-window
- Wire up seamlessly

---

## Open Items

| Item | Decision Needed |
|------|-----------------|
| `targeted_convex` | Test and report if it works |
| WebGL | Only if react-window proves insufficient |
| Config UI | Wait for RAC to be integrated |

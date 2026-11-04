## Agent skills
 
### Project
 
This repository contains a generic, deterministic Trading Card Game (TCG) engine.
 
The long-term goal is to provide a reusable simulation engine that can support:
- Any TCGs through a ruleset
- deterministic simulation
- browser/WASM applications
- desktop and mobile applications
- future graphical clients
- online multiplayer
- AI training and self-play
- replay and debugging
The engine is the source of truth for game execution.
 
### Core Architectural Principles
 
- The game state MUST be explicit and fully contained in the current game state.
- There MUST be no global mutable game state.
- Actions represent player/system intent.
- The engine validates actions before executing them.
- Events represent facts that occurred during execution.
- Effects describe game behavior and are resolved by the engine following the ruleset.
- Rules belong to a Ruleset, not to the generic engine.
- Card definitions are immutable game data.
- Card instances represent runtime state.
- Derived values should be calculated rather than duplicated as mutable state.
- The core engine MUST remain independent of UI, networking, persistence, and AI.
- Deterministic execution is a fundamental requirement.
- Prefer strong types and explicit domain concepts over generic primitives.
- Avoid premature abstractions. Let the domain emerge through incremental development.
### Domain Model
 
The current conceptual model is:
 
Game:
 
→ GameState
→ Players
→ Zones
→ Card Instances
→ Effects / Modifiers
 
Execution follows:
 
Action:
 
→ Validation
→ Execution
→ Events
→ New GameState
 
Card behavior follows:
 
Card:
→ Effects
→ Ruleset / Engine
→ New GameState
 
These are conceptual models, not implementation requirements. Do not assume specific implementation unless stated otherwise.
 
### Development Philosophy
 
Do not attempt to design the entire engine upfront.
 
Prefer:
 
understand → question → decide → document → implement → validate
 
over:
 
design everything → implement everything
 
When a design decision is uncertain, surface the uncertainty and investigate it before implementing.
 
Do not invent requirements merely to make implementation easier.
 
Plan the steps and tasks before development.
 
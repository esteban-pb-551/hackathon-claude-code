# Process Notes

## /onboard
- **Technical experience:** Intermediate developer. Strong in Rust and AWS. Some exposure to Copilot and Claude Code but not heavy usage.
- **Learning goals:** Wants to deeply understand spec-driven development as a practical workflow.
- **Creative sensibility:** Enjoys HBO's *Rome* (2005) — appreciates narrative depth, structure, and complexity. Suggests preference for substantive, well-crafted work.
- **Prior SDD experience:** Has used Kiro (Amazon's spec-driven IDE), so has baseline familiarity with planning-before-building workflows. Not starting from zero on the concept.
- **Notable context:** Came specifically to explore Claude Code's capabilities. Prefers conversation in Spanish, documents in English.
- **Energy/engagement:** Direct and focused. Gives concise answers — not a rambler. Moves at a good clip.

## /scope
- **Idea evolution:** Learner arrived with a clear technical vision from the start — Rust + VoyageAI + S3 Vectors for semantic search. The idea didn't pivot; it sharpened. Started as "an app that uses S3 Vectors to search embedded texts" and refined into a fully event-driven serverless architecture (S3 → EventBridge → Lambda → embed → S3 Vectors).
- **Key reference:** Learner provided `files/use_voyage.rs` — a working example of the full S3 Vectors + VoyageAI flow. This grounded the entire conversation in concrete, working code rather than abstract ideas.
- **Pushback and response:** When asked about cutting scope for a short hackathon, learner corrected the timeline — 29 hours, not 3-4. This expanded what's realistic. When challenged on file format support, learner made a clean cut: TXT only, PDF/CSV are future work. Decisive.
- **Architecture came from the learner:** The EventBridge → Lambda pipeline, the "prefix = index name" convention, the two-Lambda design (ingestion + search), Cargo Lambda deployment — all learner-driven. Agent didn't suggest the architecture.
- **Research resonance:** The observation that nothing like this exists in Rust + S3 Vectors (vs Python-based tools like LlamaIndex/LangChain) resonated as a differentiator.
- **Deepening rounds:** 0 rounds. Learner chose to proceed directly after mandatory questions. Concise and confident in the scope — didn't need additional refinement.
- **Active shaping:** Very high. Learner drove the architecture, tech choices, and scope cuts. Corrected agent assumptions (timeline, mongodb-voyageai purpose, file format scope). Agent followed the learner's lead throughout.
- **Language note:** Confirmed all code, Lambda names, comments, and documentation in English. Conversation in Spanish.

## /prd
- **Architecture evolution:** The learner refined the ingestion architecture significantly from the scope doc. The original "Ingestion Lambda" split into two: CheckS3Vectors (Python, Durable Function with Step-based retry logic) and EmbedS3Vectors (Rust). This wasn't a pivot — it was a sharpening of how the system actually works.
- **Durable Functions insight:** The learner drove the decision to use Lambda Durable Functions for CheckS3Vectors, citing the retry pattern (3 attempts) as a natural solution for concurrent index creation race conditions. This was learner-initiated, not agent-suggested.
- **Priority staging:** Learner organized the project into three explicit etapas with clear priorities: Etapa 1 (ingestion backend + SAM + docs), Etapa 2 (search with LLM), Etapa 3 (frontend). Etapas 1 and 2 are the submission minimum; Etapa 3 is stretch.
- **"What if" moments:** The concurrent index creation race condition was already anticipated by the learner — they had the Durable Functions retry pattern ready. The empty file case and non-.txt file upload were quickly resolved. No major surprises, which reflects the learner's strong architectural thinking.
- **Scope guard:** Etapa 3 (frontend) naturally sorted into "what we'd add with more time." The learner identified a real open problem — response times potentially exceeding 60 seconds — and was comfortable leaving it unresolved for now.
- **Search architecture addition:** SearchS3Vectors does RAG — semantic search + LLM response generation via GLM-5 (zai-org). Returns only the LLM response, not source fragments. Clear error handling: index not found → error (no LLM call), no results → fixed message (no LLM call).
- **Documentation requirements:** Learner was specific — Python best practices for CheckS3Vectors, Rust doc comments for EmbedS3Vectors, README with SAM deployment steps, explicitly no project tree in README.
- **Deepening rounds:** 0 rounds. Learner chose to proceed directly after mandatory questions. Consistent with /scope behavior — concise, confident, decisive.
- **Active shaping:** Very high. The architecture changes (Durable Functions, 3-etapa staging, metadata filter system, GLM-5 RAG pattern) all came from the learner. Agent surfaced edge cases but the learner had most already covered.

## /spec
- **Technical decisions:** voyage-4-large (over lite) for embedding quality; reqwest 0.13.2 [json, native-tls] for GLM-5 API call only; mongodb-voyageai confirmed as chunking + embedding client despite misleading name; boto3 from Lambda runtime (no requirements.txt for Python Lambda).
- **What the learner was confident about:** Entire architecture — carried over cleanly from PRD. Stack choices were immediate and decisive. File structure accepted on first proposal.
- **What the learner was uncertain about:** Nothing surfaced. Every question got a direct, confident answer.
- **Stack choices:** Rust (Cargo Lambda) for both heavy Lambdas, Python for orchestration, SAM for IaC, VoyageAI voyage-4-large 1024d, GLM-5 via Friendli API, reqwest for LLM HTTP call. All learner-driven.
- **Deployment:** All AWS, tests against real services, no mocks, no Devpost submission at this time.
- **Deepening rounds:** 0 rounds. Learner chose to proceed directly after mandatory questions. Consistent pattern across all three planning phases (/scope, /prd, /spec).
- **Active shaping:** Very high. Learner specified voyage-4-large (upgrading from scope's voyage-4-lite), provided the exact GLM-5 API contract unprompted, directed agent to use reference files as guide. All decisions were learner-initiated. Agent proposed file structure and data flow diagrams; learner confirmed without changes.
- **Reference files:** `files/use_voyage.rs` and `files/chunk_example.rs` were critical — they provided exact API usage patterns for S3 Vectors, VoyageAI embedding, and text chunking that went directly into the spec.

## /checklist
- **Sequencing decisions:** Learner proposed SAM template → CheckS3Vectors → EmbedS3Vectors for Etapa 1, which is the correct dependency order (infra before code, orchestrator before worker). Agent extended with Etapa 2 (SAM extension → SearchS3Vectors), Etapa 3 stretch (frontend), and cierre (README + Devpost). Learner confirmed without changes.
- **Build mode:** Autonomous. Learner chose immediately — consistent with decisive pattern across all phases.
- **Verification:** Yes, checkpoints every 3-4 items. Learner agreed to recommended cadence.
- **Comprehension checks:** N/A (autonomous mode).
- **Git cadence:** Commit after each checklist item.
- **Check-in cadence:** N/A (autonomous mode).
- **Item count:** 11 items. 4 for Etapa 1 (including deploy+test), 3 for Etapa 2 (including deploy+test), 2 for Etapa 3 stretch (including deploy+test), 2 for cierre (README + Devpost). Estimated total: 4-6 hours depending on deploy/debug time.
- **Submission planning:** Learner deferred Devpost details to after all three etapas are built. Wow moment identified as the live demo flow (upload .txt → search by meaning → LLM response). GitHub repo status not discussed — to be resolved at submission time.
- **Deepening rounds:** 0 rounds. Learner chose to proceed directly. Consistent pattern across all four planning phases (/scope, /prd, /spec, /checklist).
- **Active shaping:** Moderate. Learner drove the Etapa 1 sequencing (3 items in correct dependency order). Accepted agent's proposed extension for Etapa 2, stretch, and cierre without modification. Less hands-on than in earlier phases, which is expected — the heavy architectural decisions were already made in /scope, /prd, and /spec.

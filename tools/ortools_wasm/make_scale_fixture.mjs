// Generates sequencing-heavy fixtures. The committed fixtures top out at three
// orders on one resource, which every sequencing encoding solves identically --
// far too small to tell an MTZ relaxation apart from a native circuit. These
// put many orders in one shared window with asymmetric travel, so the ordering
// itself carries objective cost.
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));

function readArgument(flag, fallback = null) {
  const index = process.argv.indexOf(flag);
  if (index === -1 || index + 1 >= process.argv.length) {
    if (fallback !== null) return fallback;
    throw new Error(`missing argument ${flag}`);
  }
  return process.argv[index + 1];
}

const BASE_MS = Date.parse('2026-01-01T09:00:00Z');
const iso = (minutes) => new Date(BASE_MS + minutes * 60000).toISOString();

// Deterministic PRNG: fixtures must be byte-reproducible across runs and hosts,
// so Math.random() is not an option.
function makeRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state * 1664525 + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

function buildFixture({ orderCount, resourceCount, windowMinutes, seed }) {
  const random = makeRandom(seed);
  const resourceIds = Array.from({ length: resourceCount }, (_, i) => `user-${i + 1}`);
  const orders = [];
  const stands = [];

  for (let index = 0; index < orderCount; index += 1) {
    const orderId = `order-${index + 1}`;
    const standId = `stand-${index % Math.max(1, Math.ceil(orderCount / 2)) + 1}`;
    stands.push(standId);
    // Wide [earliest, latest] so the solver -- not the time window -- picks the
    // sequence, and every order is reachable in any position.
    orders.push({
      order_id: orderId,
      flight_id: `flight-${(index % 3) + 1}`,
      status: 'pending',
      conflict_state: 'gap',
      order_class: 'unassigned',
      planned_start_time: iso(0),
      planned_end_time: iso(10),
      earliest_start_time: iso(0),
      latest_start_time: iso(windowMinutes - 10),
      sla_deadline_time: iso(windowMinutes),
      duration_minutes: 10,
      stand_id: standId,
      current_assignment: {
        assignee_type: null,
        team_id: null,
        individual_user_id: null,
        member_user_ids: [],
        equipment_ids: [],
        task_crew: { members: [] },
      },
      baseline_assignment: {
        assignee_type: null,
        team_id: null,
        individual_user_id: null,
        member_user_ids: [],
        equipment_ids: [],
        task_crew: { members: [] },
        personnel_slot_assignments: [],
        equipment_slot_assignments: [],
      },
      personnel_slots: [
        {
          slot_code: 'lead',
          qualification_code: 'svc',
          qualification_level_code: 'L1',
          candidate_user_ids: resourceIds.slice(),
          baseline_user_id: null,
          scarcity_cost: 0,
        },
      ],
      equipment_slots: [],
    });
  }

  // Asymmetric travel: cost(i -> j) != cost(j -> i). A symmetric matrix would
  // make many orderings tie, hiding real differences between encodings.
  const travelEdges = [];
  for (const resourceId of resourceIds) {
    for (let from = 0; from < orderCount; from += 1) {
      for (let to = 0; to < orderCount; to += 1) {
        if (from === to) continue;
        const base = stands[from] === stands[to] ? 1 : 3 + Math.floor(random() * 8);
        travelEdges.push({
          resource_type: 'employee',
          resource_id: resourceId,
          from_node: `order:order-${from + 1}`,
          to_node: `order:order-${to + 1}`,
          travel_minutes: from < to ? base : base + 2,
        });
      }
    }
  }

  return {
    cluster_id: `fixture-scale-${orderCount}o-${resourceCount}r`,
    model_version: 'dispatch_wasm_pdf_full_model_v2',
    solver_version: 'dispatch_solver_ortools_wasm_strict_pdf_v3',
    objective_config: { timeout_ms: 20000 },
    optimizable_orders: orders,
    fixed_anchor_orders: [],
    employee_anchor_states: [],
    equipment_anchor_states: [],
    employee_free_windows: resourceIds.map((resourceId) => ({
      resource_type: 'employee',
      resource_id: resourceId,
      window_start: iso(0),
      window_end: iso(windowMinutes),
    })),
    equipment_free_windows: [],
    resource_travel_edges: travelEdges,
    turnaround_pairs: [],
  };
}

async function main() {
  const orderCount = Number(readArgument('--orders', '12'));
  const resourceCount = Number(readArgument('--resources', '2'));
  const windowMinutes = Number(readArgument('--window', '600'));
  const seed = Number(readArgument('--seed', '7'));
  const outputPath = path.resolve(
    readArgument(
      '--output',
      path.join(SCRIPT_DIR, 'fixtures-scale', `scale_${orderCount}o_${resourceCount}r.json`),
    ),
  );
  const request = buildFixture({ orderCount, resourceCount, windowMinutes, seed });
  await fs.mkdir(path.dirname(outputPath), { recursive: true });
  await fs.writeFile(outputPath, `${JSON.stringify({ request }, null, 2)}\n`, 'utf8');
  console.log(
    `${outputPath}  orders=${orderCount} resources=${resourceCount} travel_edges=${request.resource_travel_edges.length}`,
  );
}

main().catch((error) => {
  console.error(error?.stack || error?.message || String(error));
  process.exit(1);
});

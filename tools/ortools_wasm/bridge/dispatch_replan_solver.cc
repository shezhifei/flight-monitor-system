#include <algorithm>
#include <cmath>
#include <cstdint>
#include <map>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <emscripten/bind.h>
#include <emscripten/val.h>

#include "ortools/sat/cp_model.h"
#include "ortools/sat/cp_model_solver.h"
#include "ortools/sat/sat_parameters.pb.h"

namespace {

using emscripten::val;
using operations_research::Domain;
using operations_research::sat::BoolVar;
using operations_research::sat::CircuitConstraint;
using operations_research::sat::CpModelBuilder;
using operations_research::sat::CpSolverResponse;
using operations_research::sat::CpSolverStatus;
using operations_research::sat::IntervalVar;
using operations_research::sat::IntVar;
using operations_research::sat::LinearExpr;
using operations_research::sat::Model;
using operations_research::sat::NewSatParameters;
using operations_research::sat::SatParameters;
using operations_research::sat::SolutionBooleanValue;
using operations_research::sat::SolutionIntegerValue;
using operations_research::sat::SolveCpModel;

constexpr const char* kSolverVersion = "dispatch_solver_ortools_wasm_strict_pdf_v3";
constexpr const char* kSolverBackend = "ortools_cp_sat_wasm";
constexpr int64_t kDefaultTimeoutMs = 10000;
constexpr int64_t kMinimumOrderDurationMinutes = 5;

struct CrewMember {
  std::optional<std::string> user_id;
  std::optional<std::string> username;
  std::optional<std::string> source_team_id;
  std::optional<std::string> source_team_name;
  std::optional<std::string> slot_code;
  std::optional<std::string> qualification_code;
  std::optional<std::string> qualification_level_code;
};

struct Assignment {
  std::optional<std::string> assignee_type;
  std::optional<std::string> team_id;
  std::optional<std::string> individual_user_id;
  std::vector<std::string> equipment_ids;
  std::vector<std::string> member_user_ids;
  std::optional<std::string> department_rule_version;
  std::string crew_requirement_snapshot_json = "[]";
  std::string equipment_requirement_snapshot_json = "[]";
  std::string qualification_gap_json = "[]";
  std::vector<CrewMember> task_crew_members;
  std::vector<std::string> task_crew_source_team_ids;
  std::vector<std::string> task_crew_source_team_names;
  std::optional<std::string> task_crew_generated_from;
};

struct PersonnelSlot {
  std::string slot_code;
  std::optional<std::string> qualification_code;
  std::optional<std::string> qualification_level_code;
  std::vector<std::string> candidate_user_ids;
  std::optional<std::string> baseline_user_id;
  int64_t workload_weight = 1;
  int64_t scarcity_cost = 0;
};

struct EquipmentSlot {
  std::string slot_code;
  std::optional<std::string> equipment_type_id;
  std::vector<std::string> candidate_equipment_ids;
  std::optional<std::string> baseline_equipment_id;
};

struct BaselinePersonnelSlotAssignment {
  std::string slot_code;
  std::optional<std::string> user_id;
  std::optional<std::string> username;
  std::optional<std::string> source_team_id;
  std::optional<std::string> source_team_name;
  std::optional<std::string> qualification_code;
  std::optional<std::string> qualification_level_code;
};

struct BaselineEquipmentSlotAssignment {
  std::string slot_code;
  std::optional<std::string> equipment_id;
  std::optional<std::string> code;
  std::optional<std::string> equipment_type_id;
};

struct BaselineAssignment {
  std::optional<std::string> assignee_type;
  std::optional<std::string> team_id;
  std::optional<std::string> individual_user_id;
  std::vector<std::string> equipment_ids;
  std::vector<std::string> member_user_ids;
  std::optional<std::string> department_rule_version;
  std::string crew_requirement_snapshot_json = "[]";
  std::string equipment_requirement_snapshot_json = "[]";
  std::string qualification_gap_json = "[]";
  std::vector<CrewMember> task_crew_members;
  std::vector<std::string> task_crew_source_team_ids;
  std::vector<std::string> task_crew_source_team_names;
  std::optional<std::string> task_crew_generated_from;
  std::vector<BaselinePersonnelSlotAssignment> personnel_slot_assignments;
  std::vector<BaselineEquipmentSlotAssignment> equipment_slot_assignments;
};

struct OrderInput {
  std::string order_id;
  std::string flight_id;
  std::string status;
  std::string conflict_state = "none";
  std::string order_class = "unassigned";
  bool is_optimizable = false;
  bool is_fixed_anchor = false;
  bool is_locked = false;
  int64_t original_start_min = 0;
  int64_t original_end_min = 0;
  int64_t earliest_start_min = 0;
  int64_t latest_start_min = 0;
  bool has_fixed_completion_target = false;
  int64_t completion_target_min = 0;
  int64_t duration_min = 0;
  // Dense `crew size -> minutes` table, index `k` being the duration when `k`
  // personnel slots are filled. Empty means the owning department has no such
  // table configured and the duration stays the `duration_min` constant, which
  // is what every pre-bridge.4 snapshot produces.
  std::vector<int64_t> duration_by_crew_size;
  std::optional<std::string> stand_id;
  Assignment current_assignment;
  BaselineAssignment baseline_assignment;
  std::vector<PersonnelSlot> personnel_slots;
  std::vector<EquipmentSlot> equipment_slots;
};

struct ResourceWindow {
  std::string resource_type;
  std::string resource_id;
  int64_t window_start_min = 0;
  int64_t window_end_min = 0;
  std::optional<std::string> left_anchor_order_id;
  std::optional<std::string> left_anchor_stand_id;
  std::optional<std::string> right_anchor_order_id;
  std::optional<std::string> right_anchor_stand_id;
};

struct TravelEdge {
  std::string resource_type;
  std::string resource_id;
  std::string from_node;
  std::string to_node;
  int64_t travel_minutes = 0;
};

struct TurnaroundPair {
  std::string pair_key;
  std::string inbound_order_id;
  std::string outbound_order_id;
  std::string inbound_slot_code;
  std::string outbound_slot_code;
  bool hard_continuity_required = false;
  int64_t continuity_penalty_weight = 0;
  int64_t tightness_penalty = 0;
};

struct SlotCandidateDecision {
  std::string candidate_id;
  BoolVar selected;
};

struct PersonnelSlotDecision {
  PersonnelSlot slot;
  std::vector<SlotCandidateDecision> candidates;
  BoolVar gap;
};

struct EquipmentSlotDecision {
  EquipmentSlot slot;
  std::vector<SlotCandidateDecision> candidates;
  BoolVar gap;
};

struct WindowChoiceDecision {
  ResourceWindow window;
  BoolVar selected;
  BoolVar is_first;
  BoolVar is_last;
  int64_t left_anchor_travel = 0;
  int64_t right_anchor_travel = 0;
};

struct ResourceUsageDecision {
  std::string resource_type;
  std::string resource_id;
  BoolVar used;
  // Optional interval spanning this order's occupancy of the resource, present
  // iff `used`. Feeds one AddNoOverlap per resource so CP-SAT's disjunctive
  // reasoning enforces mutual exclusion directly, instead of leaving it to be
  // inferred from the per-window sequencing literals.
  IntervalVar occupancy;
  std::vector<BoolVar> selectors;
  std::vector<WindowChoiceDecision> window_choices;
};

struct OrderDecision {
  OrderInput order;
  IntVar start;
  // How long this order occupies its resources. A constant equal to
  // `order.duration_min` unless the department configured a crew-size table, in
  // which case it is an IntVar tied to the number of filled personnel slots:
  // fewer people on the job means a longer job.
  LinearExpr duration;
  // Where this order stops occupying its resources, always equal to
  // `start + duration`.
  //
  // It has to be its own expression rather than that sum written inline: an
  // interval's start, size and end each have to be affine, and once duration is
  // a variable the sum carries two variable terms and CP-SAT rejects the whole
  // model. With a constant duration this stays the affine `start + k`; with a
  // table it is a fresh IntVar pinned by an ordinary linear equality, which has
  // no such restriction.
  LinearExpr end;
  IntVar lateness;
  BoolVar time_changed;
  std::vector<PersonnelSlotDecision> personnel_slots;
  std::vector<EquipmentSlotDecision> equipment_slots;
  std::map<std::pair<std::string, std::string>, ResourceUsageDecision> resource_usages;
};

struct StageRecord {
  std::string stage;
  std::string solve_status;
  int64_t objective_value = 0;
  int64_t wall_time_ms = 0;
  int64_t conflicts = 0;
  int64_t branches = 0;
  double best_bound = 0.0;
  // False when the stage stopped on its time budget with an incumbent rather
  // than proving optimality. The lexicographic bound locked after such a stage
  // is an upper bound on the true optimum, so every later stage inherits the
  // relaxation and the overall result is only best-effort.
  bool proven_optimal = true;
  int64_t budget_ms = 0;
};

struct StageSolveResult {
  CpSolverResponse response;
  int64_t objective_value = 0;
  StageRecord record;
};

struct PersonnelSlotAssignmentResult {
  std::string dispatch_order_id;
  std::string slot_code;
  std::optional<std::string> user_id;
  std::optional<std::string> username;
  std::optional<std::string> source_team_id;
  std::optional<std::string> source_team_name;
  std::optional<std::string> qualification_code;
  std::optional<std::string> qualification_level_code;
  std::optional<std::string> baseline_user_id;
  bool changed = false;
};

struct EquipmentSlotAssignmentResult {
  std::string dispatch_order_id;
  std::string slot_code;
  std::optional<std::string> equipment_id;
  std::optional<std::string> code;
  std::optional<std::string> equipment_type_id;
  std::optional<std::string> baseline_equipment_id;
  bool changed = false;
};

struct ContinuityDecisionResult {
  std::string pair_key;
  std::string inbound_order_id;
  std::string outbound_order_id;
  std::string inbound_slot_code;
  std::string outbound_slot_code;
  bool satisfied = false;
  bool hard_continuity_required = false;
  int64_t penalty_applied = 0;
};

struct OrderResultData {
  std::string dispatch_order_id;
  std::string reason;
  std::optional<std::string> suggestion_type;
  std::string order_class;
  int64_t original_start_min = 0;
  int64_t original_end_min = 0;
  int64_t suggested_start_min = 0;
  int64_t suggested_end_min = 0;
  int64_t lateness_minutes = 0;
  int64_t gap_count = 0;
  int64_t travel_minutes = 0;
  int64_t baseline_change_count = 0;
  int64_t scarcity_cost = 0;
  int64_t load_deviation = 0;
  double impact_score = 0.0;
  bool requires_manual_confirmation = false;
  Assignment current_assignment;
  Assignment suggested_assignment;
  std::string crew_requirement_snapshot_json = "[]";
  std::string qualification_gap_json = "[]";
  std::vector<PersonnelSlotAssignmentResult> personnel_slot_assignments;
  std::vector<EquipmentSlotAssignmentResult> equipment_slot_assignments;
  std::vector<ContinuityDecisionResult> continuity_decisions;
  int64_t continuity_break_count = 0;
  bool time_changed = false;
};

struct SolveArtifacts {
  std::string cluster_id;
  std::string model_version;
  std::string solver_version;
  int64_t slot_gap = 0;
  int64_t total_lateness_minutes = 0;
  int64_t continuity_break = 0;
  int64_t continuity_penalty = 0;
  int64_t baseline_change = 0;
  int64_t travel_cost = 0;
  int64_t scarcity_cost = 0;
  int64_t load_deviation = 0;
  int64_t wall_time_ms = 0;
  int64_t conflicts = 0;
  int64_t branches = 0;
  double best_bound = 0.0;
  std::string solve_status = "OPTIMAL";
  bool feasible = true;
  // True when every personnel and equipment slot got filled.
  //
  // Distinct from `feasible`, which only says CP-SAT found a solution. An empty
  // candidate list makes `AddExactlyOne({candidates…, gap})` pin that slot's gap
  // to 1 by construction, so "nobody is available for this job" is a perfectly
  // feasible, perfectly optimal plan. Callers that want to know whether the plan
  // can actually be worked have to read this, not `feasible`.
  bool plan_complete = false;
  bool timed_out = false;
  // True when at least one lexicographic stage exhausted its budget without
  // proving optimality. The plan is still feasible and still respects every
  // hard constraint, but the staged objective order was only approximated.
  bool lexicographic_degraded = false;
  std::vector<std::string> degraded_stages;
  std::optional<std::string> error;
  std::vector<StageRecord> stage_records;
  std::vector<std::string> unresolved_assigned_conflict_order_ids;
  std::vector<std::string> unassigned_unplanned_order_ids;
  std::vector<PersonnelSlotAssignmentResult> personnel_slot_assignments;
  std::vector<EquipmentSlotAssignmentResult> equipment_slot_assignments;
  std::vector<ContinuityDecisionResult> continuity_decisions;
  std::vector<OrderResultData> order_results;
};

struct ResourcePathEdgeDecision {
  std::string resource_type;
  std::string resource_id;
  size_t window_index = 0;
  std::string from_order_id;
  std::string to_order_id;
  BoolVar selected;
  int64_t travel_minutes = 0;
};

struct TurnaroundDecision {
  TurnaroundPair pair;
  BoolVar both_non_gap;
  BoolVar violation;
  std::vector<BoolVar> same_candidate_matches;
};

struct TurnaroundEndpoint {
  const OrderInput* order = nullptr;
  PersonnelSlotDecision* optimizable_slot = nullptr;
  std::optional<BaselinePersonnelSlotAssignment> fixed_assignment;
  bool is_fixed = false;
};

class SolveFailure : public std::runtime_error {
 public:
  SolveFailure(std::string solve_status, std::string message)
      : std::runtime_error(std::move(message)), solve_status_(std::move(solve_status)) {}

  const std::string& solve_status() const { return solve_status_; }

 private:
  std::string solve_status_;
};

using ResourceKey = std::pair<std::string, std::string>;
using TravelKey = std::tuple<std::string, std::string, std::string, std::string>;
using WindowIdentity = std::tuple<std::string,
                                  std::string,
                                  int64_t,
                                  int64_t,
                                  std::string,
                                  std::string,
                                  std::string,
                                  std::string>;
using WindowLookup = std::map<ResourceKey, std::vector<ResourceWindow>>;
using TravelLookup = std::map<TravelKey, int64_t>;

bool IsNullish(const val& input) {
  return input.isNull() || input.isUndefined();
}

val ToJsNumber(int64_t value) {
  return val(static_cast<double>(value));
}

val ToJsDouble(double value) {
  return val(value);
}

std::optional<std::string> NormalizeText(const std::string& value) {
  const auto first = value.find_first_not_of(" \t\r\n");
  if (first == std::string::npos) {
    return std::nullopt;
  }
  const auto last = value.find_last_not_of(" \t\r\n");
  return value.substr(first, last - first + 1);
}

std::optional<std::string> GetOptionalString(const val& input, const char* key) {
  if (IsNullish(input)) {
    return std::nullopt;
  }
  const val value = input[key];
  if (IsNullish(value)) {
    return std::nullopt;
  }
  return NormalizeText(value.as<std::string>());
}

bool GetBoolean(const val& input, const char* key) {
  if (IsNullish(input)) {
    return false;
  }
  const val value = input[key];
  if (IsNullish(value)) {
    return false;
  }
  return value.as<bool>();
}

int64_t GetInt64(const val& input, const char* key, int64_t default_value = 0) {
  if (IsNullish(input)) {
    return default_value;
  }
  const val value = input[key];
  if (IsNullish(value)) {
    return default_value;
  }
  return static_cast<int64_t>(std::llround(value.as<double>()));
}

std::vector<val> ToValVector(const val& input) {
  std::vector<val> values;
  if (IsNullish(input)) {
    return values;
  }
  const auto length = input["length"].as<unsigned>();
  values.reserve(length);
  for (unsigned index = 0; index < length; ++index) {
    values.push_back(input[index]);
  }
  return values;
}

std::vector<std::string> NormalizeStringArray(const val& input) {
  std::vector<std::string> result;
  std::set<std::string> seen;
  for (const val& item : ToValVector(input)) {
    if (IsNullish(item)) {
      continue;
    }
    const auto normalized = NormalizeText(item.as<std::string>());
    if (!normalized.has_value()) {
      continue;
    }
    if (seen.insert(*normalized).second) {
      result.push_back(*normalized);
    }
  }
  return result;
}

// Reads the dense `crew size -> minutes` table, rejecting the whole table
// rather than repairing it entry by entry.
//
// A partially trusted duration table is worse than none: index `k` feeds the
// occupancy interval and every travel bound, so one nonsense entry silently
// moves other orders. `slot_count + 1` entries are required because the solver
// indexes it by the number of filled slots, zero included, and each must be a
// positive number of minutes. Anything else means the producer and this bridge
// disagree, and the constant duration is the honest fallback.
std::vector<int64_t> NormalizeDurationTable(const val& input, size_t slot_count) {
  std::vector<int64_t> result;
  const auto entries = ToValVector(input);
  if (entries.size() != slot_count + 1) {
    return {};
  }
  result.reserve(entries.size());
  for (const val& entry : entries) {
    if (IsNullish(entry)) {
      return {};
    }
    const double raw = entry.as<double>();
    if (!std::isfinite(raw)) {
      return {};
    }
    const int64_t minutes = static_cast<int64_t>(std::llround(raw));
    if (minutes <= 0) {
      return {};
    }
    result.push_back(std::max<int64_t>(kMinimumOrderDurationMinutes, minutes));
  }
  return result;
}

std::string JsonStringify(const val& input) {
  if (IsNullish(input)) {
    return "null";
  }
  return val::global("JSON").call<val>("stringify", input).as<std::string>();
}

val JsonParse(const std::string& input_json) {
  return val::global("JSON").call<val>("parse", val(input_json));
}

std::optional<int64_t> ParseIsoMinutes(const std::optional<std::string>& value) {
  if (!value.has_value()) {
    return std::nullopt;
  }
  const val parsed = val::global("Date").call<val>("parse", val(*value));
  const double millis = parsed.as<double>();
  if (std::isnan(millis)) {
    return std::nullopt;
  }
  return static_cast<int64_t>(std::llround(millis / 60000.0));
}

std::optional<std::string> MinutesToIso(int64_t value) {
  const auto millis = static_cast<double>(value) * 60000.0;
  const val date = val::global("Date").new_(val(millis));
  return date.call<val>("toISOString").as<std::string>();
}

std::string StatusToString(CpSolverStatus status) {
  switch (status) {
    case CpSolverStatus::OPTIMAL:
      return "OPTIMAL";
    case CpSolverStatus::FEASIBLE:
      return "FEASIBLE";
    case CpSolverStatus::INFEASIBLE:
      return "INFEASIBLE";
    case CpSolverStatus::MODEL_INVALID:
      return "MODEL_INVALID";
    default:
      return "UNKNOWN";
  }
}

BoolVar FixedBool(CpModelBuilder* model_builder, bool value, const std::string& name) {
  const BoolVar literal = model_builder->NewBoolVar().WithName(name);
  model_builder->FixVariable(literal, value);
  return literal;
}

LinearExpr SumBoolVars(const std::vector<BoolVar>& vars) {
  LinearExpr expression;
  for (const BoolVar var : vars) {
    expression += var;
  }
  return expression;
}

bool HasPrimaryAssignment(const Assignment& assignment) {
  return assignment.individual_user_id.has_value() || !assignment.member_user_ids.empty();
}

bool HasAnyAssignedResource(const Assignment& assignment) {
  return HasPrimaryAssignment(assignment) || !assignment.equipment_ids.empty();
}

bool HasPrimaryAssignment(const BaselineAssignment& assignment) {
  return assignment.individual_user_id.has_value() || !assignment.member_user_ids.empty();
}

bool HasAnyAssignedResource(const BaselineAssignment& assignment) {
  return HasPrimaryAssignment(assignment) || !assignment.equipment_ids.empty();
}

CrewMember ParseCrewMember(const val& input) {
  CrewMember member;
  member.user_id = GetOptionalString(input, "user_id");
  member.username = GetOptionalString(input, "username");
  member.source_team_id = GetOptionalString(input, "source_team_id");
  member.source_team_name = GetOptionalString(input, "source_team_name");
  member.slot_code = GetOptionalString(input, "slot_code");
  member.qualification_code = GetOptionalString(input, "qualification_code");
  member.qualification_level_code = GetOptionalString(input, "qualification_level_code");
  return member;
}

Assignment ParseAssignment(const val& input) {
  Assignment assignment;
  assignment.assignee_type = GetOptionalString(input, "assignee_type");
  assignment.team_id = GetOptionalString(input, "team_id");
  assignment.individual_user_id = GetOptionalString(input, "individual_user_id");
  assignment.equipment_ids = NormalizeStringArray(input["equipment_ids"]);
  assignment.member_user_ids = NormalizeStringArray(input["member_user_ids"]);
  assignment.department_rule_version = GetOptionalString(input, "department_rule_version");
  assignment.crew_requirement_snapshot_json = JsonStringify(input["crew_requirement_snapshot"]);
  assignment.equipment_requirement_snapshot_json =
      JsonStringify(input["equipment_requirement_snapshot"]);
  assignment.qualification_gap_json = JsonStringify(input["qualification_gap"]);

  const val task_crew = input["task_crew"];
  assignment.task_crew_source_team_ids = NormalizeStringArray(task_crew["source_team_ids"]);
  assignment.task_crew_source_team_names = NormalizeStringArray(task_crew["source_team_names"]);
  assignment.task_crew_generated_from = GetOptionalString(task_crew, "generated_from");
  for (const val& member_val : ToValVector(task_crew["members"])) {
    assignment.task_crew_members.push_back(ParseCrewMember(member_val));
  }
  return assignment;
}

BaselinePersonnelSlotAssignment ParseBaselinePersonnelSlotAssignment(const val& input) {
  const auto slot_code = GetOptionalString(input, "slot_code");
  if (!slot_code.has_value()) {
    throw std::runtime_error("baseline personnel slot missing slot_code");
  }
  BaselinePersonnelSlotAssignment assignment;
  assignment.slot_code = *slot_code;
  assignment.user_id = GetOptionalString(input, "user_id");
  assignment.username = GetOptionalString(input, "username");
  assignment.source_team_id = GetOptionalString(input, "source_team_id");
  assignment.source_team_name = GetOptionalString(input, "source_team_name");
  assignment.qualification_code = GetOptionalString(input, "qualification_code");
  assignment.qualification_level_code = GetOptionalString(input, "qualification_level_code");
  return assignment;
}

BaselineEquipmentSlotAssignment ParseBaselineEquipmentSlotAssignment(const val& input) {
  const auto slot_code = GetOptionalString(input, "slot_code");
  if (!slot_code.has_value()) {
    throw std::runtime_error("baseline equipment slot missing slot_code");
  }
  BaselineEquipmentSlotAssignment assignment;
  assignment.slot_code = *slot_code;
  assignment.equipment_id = GetOptionalString(input, "equipment_id");
  assignment.code = GetOptionalString(input, "code");
  assignment.equipment_type_id = GetOptionalString(input, "equipment_type_id");
  return assignment;
}

BaselineAssignment ParseBaselineAssignment(const val& input) {
  BaselineAssignment baseline;
  baseline.assignee_type = GetOptionalString(input, "assignee_type");
  baseline.team_id = GetOptionalString(input, "team_id");
  baseline.individual_user_id = GetOptionalString(input, "individual_user_id");
  baseline.equipment_ids = NormalizeStringArray(input["equipment_ids"]);
  baseline.member_user_ids = NormalizeStringArray(input["member_user_ids"]);
  baseline.department_rule_version = GetOptionalString(input, "department_rule_version");
  baseline.crew_requirement_snapshot_json = JsonStringify(input["crew_requirement_snapshot"]);
  baseline.equipment_requirement_snapshot_json =
      JsonStringify(input["equipment_requirement_snapshot"]);
  baseline.qualification_gap_json = JsonStringify(input["qualification_gap"]);

  const val task_crew = input["task_crew"];
  baseline.task_crew_source_team_ids = NormalizeStringArray(task_crew["source_team_ids"]);
  baseline.task_crew_source_team_names = NormalizeStringArray(task_crew["source_team_names"]);
  baseline.task_crew_generated_from = GetOptionalString(task_crew, "generated_from");
  for (const val& member_val : ToValVector(task_crew["members"])) {
    baseline.task_crew_members.push_back(ParseCrewMember(member_val));
  }
  for (const val& item : ToValVector(input["personnel_slot_assignments"])) {
    baseline.personnel_slot_assignments.push_back(ParseBaselinePersonnelSlotAssignment(item));
  }
  for (const val& item : ToValVector(input["equipment_slot_assignments"])) {
    baseline.equipment_slot_assignments.push_back(ParseBaselineEquipmentSlotAssignment(item));
  }
  return baseline;
}

PersonnelSlot ParsePersonnelSlot(const val& input) {
  const auto slot_code = GetOptionalString(input, "slot_code");
  if (!slot_code.has_value()) {
    throw std::runtime_error("personnel slot missing slot_code");
  }
  PersonnelSlot slot;
  slot.slot_code = *slot_code;
  slot.qualification_code = GetOptionalString(input, "qualification_code");
  slot.qualification_level_code = GetOptionalString(input, "qualification_level_code");
  slot.candidate_user_ids = NormalizeStringArray(input["candidate_user_ids"]);
  slot.baseline_user_id = GetOptionalString(input, "baseline_user_id");
  slot.workload_weight = std::max<int64_t>(1, GetInt64(input, "workload_weight", 1));
  slot.scarcity_cost = std::max<int64_t>(0, GetInt64(input, "scarcity_cost", 0));
  return slot;
}

EquipmentSlot ParseEquipmentSlot(const val& input) {
  const auto slot_code = GetOptionalString(input, "slot_code");
  if (!slot_code.has_value()) {
    throw std::runtime_error("equipment slot missing slot_code");
  }
  EquipmentSlot slot;
  slot.slot_code = *slot_code;
  slot.equipment_type_id = GetOptionalString(input, "equipment_type_id");
  slot.candidate_equipment_ids = NormalizeStringArray(input["candidate_equipment_ids"]);
  slot.baseline_equipment_id = GetOptionalString(input, "baseline_equipment_id");
  return slot;
}

OrderInput ParseOrderInput(const val& input) {
  const auto order_id = GetOptionalString(input, "order_id");
  if (!order_id.has_value()) {
    throw std::runtime_error("order_id is required");
  }

  const auto planned_start = ParseIsoMinutes(GetOptionalString(input, "planned_start_time"));
  const auto planned_end = ParseIsoMinutes(GetOptionalString(input, "planned_end_time"));
  const auto earliest_start = ParseIsoMinutes(GetOptionalString(input, "earliest_start_time"));
  const auto latest_start = ParseIsoMinutes(GetOptionalString(input, "latest_start_time"));
  const auto completion_target =
      ParseIsoMinutes(GetOptionalString(input, "completion_target_time"));
  const auto required_start = ParseIsoMinutes(GetOptionalString(input, "required_start_time"));
  const auto actual_start = ParseIsoMinutes(GetOptionalString(input, "actual_start_time"));
  const auto actual_end = ParseIsoMinutes(GetOptionalString(input, "actual_end_time"));
  const auto estimated_completion =
      ParseIsoMinutes(GetOptionalString(input, "estimated_completion_time"));
  const auto effective_start = ParseIsoMinutes(GetOptionalString(input, "effective_start_time"));
  const auto effective_end = ParseIsoMinutes(GetOptionalString(input, "effective_end_time"));

  const int64_t original_start_min =
      planned_start.value_or(effective_start.value_or(actual_start.value_or(0)));
  if (original_start_min <= 0) {
    throw std::runtime_error("missing start time for " + *order_id);
  }
  const int64_t original_end_min = std::max(
      planned_end.value_or(effective_end.value_or(
          estimated_completion.value_or(actual_end.value_or(original_start_min + 15)))),
      original_start_min + kMinimumOrderDurationMinutes);
  const int64_t order_earliest_start =
      earliest_start.value_or(required_start.value_or(planned_start.value_or(original_start_min)));
  const int64_t order_latest_start =
      std::max<int64_t>(order_earliest_start,
                        latest_start.value_or(required_start.value_or(planned_start.value_or(original_start_min))));

  OrderInput order;
  order.order_id = *order_id;
  order.flight_id = GetOptionalString(input, "flight_id").value_or("");
  order.status = GetOptionalString(input, "status").value_or("pending");
  order.conflict_state = GetOptionalString(input, "conflict_state").value_or("none");
  order.order_class = GetOptionalString(input, "order_class").value_or(
      order.conflict_state == "resource_conflict" ? "assigned_conflict"
                                                  : (order.conflict_state == "gap" ? "unassigned"
                                                                                   : "locked"));
  order.is_optimizable = GetBoolean(input, "is_optimizable");
  order.is_fixed_anchor = GetBoolean(input, "is_fixed_anchor");
  order.is_locked = GetBoolean(input, "is_locked") || order.order_class == "locked";
  order.original_start_min = original_start_min;
  order.original_end_min = original_end_min;
  order.earliest_start_min = order_earliest_start;
  order.latest_start_min = order_latest_start;
  order.has_fixed_completion_target =
      GetOptionalString(input, "completion_time_mode").value_or("") ==
          "completion_anchor_offset" &&
      completion_target.has_value();
  order.completion_target_min = completion_target.value_or(original_end_min);
  order.duration_min = std::max<int64_t>(kMinimumOrderDurationMinutes,
                                         original_end_min - original_start_min);
  order.stand_id = GetOptionalString(input, "stand_id");
  order.current_assignment = ParseAssignment(input["current_assignment"]);
  order.baseline_assignment = ParseBaselineAssignment(input["baseline_assignment"]);
  for (const val& slot : ToValVector(input["personnel_slots"])) {
    order.personnel_slots.push_back(ParsePersonnelSlot(slot));
  }
  for (const val& slot : ToValVector(input["equipment_slots"])) {
    order.equipment_slots.push_back(ParseEquipmentSlot(slot));
  }
  // Read after the slots: the table is indexed by how many personnel slots end
  // up filled, so its expected length is only known once they are parsed.
  order.duration_by_crew_size =
      NormalizeDurationTable(input["duration_by_crew_size"], order.personnel_slots.size());
  return order;
}

ResourceWindow ParseResourceWindow(const val& input) {
  const auto resource_type = GetOptionalString(input, "resource_type");
  const auto resource_id = GetOptionalString(input, "resource_id");
  const auto window_start = ParseIsoMinutes(GetOptionalString(input, "window_start"));
  const auto window_end = ParseIsoMinutes(GetOptionalString(input, "window_end"));
  if (!resource_type.has_value() || !resource_id.has_value() || !window_start.has_value() ||
      !window_end.has_value()) {
    throw std::runtime_error("free window is missing required fields");
  }
  ResourceWindow window;
  window.resource_type = *resource_type;
  window.resource_id = *resource_id;
  window.window_start_min = *window_start;
  window.window_end_min = std::max(*window_end, *window_start);
  window.left_anchor_order_id = GetOptionalString(input, "left_anchor_order_id");
  window.left_anchor_stand_id = GetOptionalString(input, "left_anchor_stand_id");
  window.right_anchor_order_id = GetOptionalString(input, "right_anchor_order_id");
  window.right_anchor_stand_id = GetOptionalString(input, "right_anchor_stand_id");
  return window;
}

WindowIdentity WindowIdentityFor(const ResourceWindow& window) {
  return {window.resource_type,
          window.resource_id,
          window.window_start_min,
          window.window_end_min,
          window.left_anchor_order_id.value_or(""),
          window.left_anchor_stand_id.value_or(""),
          window.right_anchor_order_id.value_or(""),
          window.right_anchor_stand_id.value_or("")};
}

void PushUniqueResourceWindow(std::vector<ResourceWindow>* windows,
                              std::set<WindowIdentity>* seen_windows,
                              const ResourceWindow& window) {
  const WindowIdentity identity = WindowIdentityFor(window);
  if (seen_windows->insert(identity).second) {
    windows->push_back(window);
  }
}

void AppendAnchorStateWindows(const val& anchor_states,
                              std::vector<ResourceWindow>* windows,
                              std::set<WindowIdentity>* seen_windows) {
  for (const val& anchor_state : ToValVector(anchor_states)) {
    const auto resource_type = GetOptionalString(anchor_state, "resource_type");
    const auto resource_id = GetOptionalString(anchor_state, "resource_id");
    if (!resource_type.has_value() || !resource_id.has_value()) {
      throw std::runtime_error("anchor state is missing resource_type or resource_id");
    }
    for (const val& free_window : ToValVector(anchor_state["free_windows"])) {
      const auto window_start = ParseIsoMinutes(GetOptionalString(free_window, "window_start"));
      const auto window_end = ParseIsoMinutes(GetOptionalString(free_window, "window_end"));
      if (!window_start.has_value() || !window_end.has_value()) {
        throw std::runtime_error("anchor state free window is missing window_start or window_end");
      }
      ResourceWindow window;
      window.resource_type =
          GetOptionalString(free_window, "resource_type").value_or(*resource_type);
      window.resource_id = GetOptionalString(free_window, "resource_id").value_or(*resource_id);
      window.window_start_min = *window_start;
      window.window_end_min = std::max(*window_end, *window_start);
      window.left_anchor_order_id = GetOptionalString(free_window, "left_anchor_order_id");
      window.left_anchor_stand_id = GetOptionalString(free_window, "left_anchor_stand_id");
      window.right_anchor_order_id = GetOptionalString(free_window, "right_anchor_order_id");
      window.right_anchor_stand_id = GetOptionalString(free_window, "right_anchor_stand_id");
      PushUniqueResourceWindow(windows, seen_windows, window);
    }
  }
}

TravelEdge ParseTravelEdge(const val& input) {
  const auto resource_type = GetOptionalString(input, "resource_type");
  const auto resource_id = GetOptionalString(input, "resource_id");
  const auto from_node = GetOptionalString(input, "from_node");
  const auto to_node = GetOptionalString(input, "to_node");
  if (!resource_type.has_value() || !resource_id.has_value() || !from_node.has_value() ||
      !to_node.has_value()) {
    throw std::runtime_error("travel edge is missing required fields");
  }
  TravelEdge edge;
  edge.resource_type = *resource_type;
  edge.resource_id = *resource_id;
  edge.from_node = *from_node;
  edge.to_node = *to_node;
  edge.travel_minutes = std::max<int64_t>(0, GetInt64(input, "travel_minutes", 0));
  return edge;
}

TurnaroundPair ParseTurnaroundPair(const val& input) {
  const auto pair_key = GetOptionalString(input, "pair_key");
  const auto inbound_order_id = GetOptionalString(input, "inbound_order_id");
  const auto outbound_order_id = GetOptionalString(input, "outbound_order_id");
  auto inbound_slot_code = GetOptionalString(input, "inbound_slot_code");
  auto outbound_slot_code = GetOptionalString(input, "outbound_slot_code");
  const std::vector<val> slot_pairs = ToValVector(input["slot_pairs"]);
  if ((!inbound_slot_code.has_value() || !outbound_slot_code.has_value()) && !slot_pairs.empty()) {
    inbound_slot_code = GetOptionalString(slot_pairs.front(), "inbound_slot_code");
    outbound_slot_code = GetOptionalString(slot_pairs.front(), "outbound_slot_code");
  }
  if (!pair_key.has_value() || !inbound_order_id.has_value() || !outbound_order_id.has_value() ||
      !inbound_slot_code.has_value() || !outbound_slot_code.has_value()) {
    throw std::runtime_error("turnaround pair is missing required fields");
  }
  TurnaroundPair pair;
  pair.pair_key = *pair_key;
  pair.inbound_order_id = *inbound_order_id;
  pair.outbound_order_id = *outbound_order_id;
  pair.inbound_slot_code = *inbound_slot_code;
  pair.outbound_slot_code = *outbound_slot_code;
  pair.hard_continuity_required = GetBoolean(input, "hard_continuity_required");
  pair.continuity_penalty_weight = std::max<int64_t>(
      0, GetInt64(input, "continuity_penalty_weight", pair.hard_continuity_required ? 1000 : 100));
  pair.tightness_penalty = std::max<int64_t>(
      0, GetInt64(input, "tightness_penalty", pair.continuity_penalty_weight));
  return pair;
}

WindowLookup BuildWindowLookup(const std::vector<ResourceWindow>& windows) {
  WindowLookup lookup;
  for (const auto& window : windows) {
    lookup[{window.resource_type, window.resource_id}].push_back(window);
  }
  for (auto& entry : lookup) {
    std::sort(entry.second.begin(), entry.second.end(),
              [](const ResourceWindow& left, const ResourceWindow& right) {
                return std::make_tuple(left.window_start_min,
                                       left.window_end_min,
                                       left.left_anchor_order_id.value_or(""),
                                       left.right_anchor_order_id.value_or("")) <
                       std::make_tuple(right.window_start_min,
                                       right.window_end_min,
                                       right.left_anchor_order_id.value_or(""),
                                       right.right_anchor_order_id.value_or(""));
              });
  }
  return lookup;
}

TravelLookup BuildTravelLookup(const std::vector<TravelEdge>& edges) {
  TravelLookup lookup;
  for (const auto& edge : edges) {
    lookup[{edge.resource_type, edge.resource_id, edge.from_node, edge.to_node}] =
        std::max<int64_t>(0, edge.travel_minutes);
  }
  return lookup;
}

std::string AnchorNode(const std::string& resource_type, const std::string& resource_id) {
  return "anchor:" + resource_type + ":" + resource_id;
}

std::string OrderNode(const std::string& order_id) {
  return "order:" + order_id;
}

std::optional<int64_t> LookupTravelMinutes(const TravelLookup& lookup,
                                           const std::string& resource_type,
                                           const std::string& resource_id,
                                           const std::string& from_node,
                                           const std::string& to_node) {
  const auto iterator = lookup.find({resource_type, resource_id, from_node, to_node});
  if (iterator == lookup.end()) {
    return std::nullopt;
  }
  return iterator->second;
}

int64_t RequireTravelMinutes(const TravelLookup& lookup,
                             const std::string& resource_type,
                             const std::string& resource_id,
                             const std::string& from_node,
                             const std::string& to_node,
                             const std::string& context) {
  const auto travel = LookupTravelMinutes(lookup, resource_type, resource_id, from_node, to_node);
  if (!travel.has_value()) {
    throw std::runtime_error("invalid model: missing travel edge for " + context + " (" +
                             resource_type + ":" + resource_id + " " + from_node + " -> " +
                             to_node + ")");
  }
  return *travel;
}

std::optional<CrewMember> FindCurrentCrewMember(const OrderInput& order,
                                                const std::string& slot_code,
                                                const std::string& user_id) {
  for (const auto& member : order.current_assignment.task_crew_members) {
    if (member.user_id == std::optional<std::string>(user_id) &&
        member.slot_code == std::optional<std::string>(slot_code)) {
      return member;
    }
  }
  for (const auto& member : order.current_assignment.task_crew_members) {
    if (member.user_id == std::optional<std::string>(user_id)) {
      return member;
    }
  }
  return std::nullopt;
}

std::optional<BaselinePersonnelSlotAssignment> FindBaselinePersonnelSlotAssignment(
    const OrderInput& order, const std::string& slot_code) {
  for (const auto& assignment : order.baseline_assignment.personnel_slot_assignments) {
    if (assignment.slot_code == slot_code) {
      return assignment;
    }
  }
  return std::nullopt;
}

std::optional<BaselineEquipmentSlotAssignment> FindBaselineEquipmentSlotAssignment(
    const OrderInput& order, const std::string& slot_code) {
  for (const auto& assignment : order.baseline_assignment.equipment_slot_assignments) {
    if (assignment.slot_code == slot_code) {
      return assignment;
    }
  }
  return std::nullopt;
}

Assignment BuildSuggestedAssignment(
    const OrderInput& order,
    const std::vector<PersonnelSlotAssignmentResult>& personnel_slot_assignments,
    const std::vector<EquipmentSlotAssignmentResult>& equipment_slot_assignments) {
  Assignment suggested;
  if (order.baseline_assignment.task_crew_members.empty() &&
      !HasAnyAssignedResource(order.baseline_assignment)) {
    suggested = order.current_assignment;
  } else {
    suggested.assignee_type = order.baseline_assignment.assignee_type;
    suggested.team_id = order.baseline_assignment.team_id;
    suggested.individual_user_id = order.baseline_assignment.individual_user_id;
    suggested.department_rule_version = order.baseline_assignment.department_rule_version;
    suggested.crew_requirement_snapshot_json =
        order.baseline_assignment.crew_requirement_snapshot_json;
    suggested.equipment_requirement_snapshot_json =
        order.baseline_assignment.equipment_requirement_snapshot_json;
    suggested.qualification_gap_json = "[]";
    suggested.task_crew_generated_from = std::optional<std::string>("ortools_wasm");
  }
  if (suggested.department_rule_version == std::nullopt) {
    suggested.department_rule_version = order.current_assignment.department_rule_version;
  }
  if (suggested.crew_requirement_snapshot_json == "null") {
    suggested.crew_requirement_snapshot_json = order.current_assignment.crew_requirement_snapshot_json;
  }
  if (suggested.equipment_requirement_snapshot_json == "null") {
    suggested.equipment_requirement_snapshot_json =
        order.current_assignment.equipment_requirement_snapshot_json;
  }

  std::set<std::string> member_ids_seen;
  std::set<std::string> team_ids_seen;
  std::set<std::string> team_names_seen;
  for (const auto& slot_assignment : personnel_slot_assignments) {
    if (!slot_assignment.user_id.has_value()) {
      continue;
    }
    CrewMember member;
    member.user_id = slot_assignment.user_id;
    member.username = slot_assignment.username;
    member.source_team_id = slot_assignment.source_team_id;
    member.source_team_name = slot_assignment.source_team_name;
    member.slot_code = slot_assignment.slot_code;
    member.qualification_code = slot_assignment.qualification_code;
    member.qualification_level_code = slot_assignment.qualification_level_code;
    suggested.task_crew_members.push_back(member);
    if (member_ids_seen.insert(*slot_assignment.user_id).second) {
      suggested.member_user_ids.push_back(*slot_assignment.user_id);
    }
    if (slot_assignment.source_team_id.has_value() &&
        team_ids_seen.insert(*slot_assignment.source_team_id).second) {
      suggested.task_crew_source_team_ids.push_back(*slot_assignment.source_team_id);
    }
    if (slot_assignment.source_team_name.has_value() &&
        team_names_seen.insert(*slot_assignment.source_team_name).second) {
      suggested.task_crew_source_team_names.push_back(*slot_assignment.source_team_name);
    }
    if (slot_assignment.slot_code == "primary") {
      suggested.individual_user_id = slot_assignment.user_id;
      if (slot_assignment.source_team_id.has_value()) {
        suggested.team_id = slot_assignment.source_team_id;
      }
      suggested.assignee_type = std::optional<std::string>("individual");
    }
  }
  if (!suggested.individual_user_id.has_value()) {
    for (const auto& slot_assignment : personnel_slot_assignments) {
      if (slot_assignment.user_id.has_value()) {
        suggested.individual_user_id = slot_assignment.user_id;
        if (slot_assignment.source_team_id.has_value()) {
          suggested.team_id = slot_assignment.source_team_id;
        }
        suggested.assignee_type = std::optional<std::string>("individual");
        break;
      }
    }
  }

  for (const auto& slot_assignment : equipment_slot_assignments) {
    if (slot_assignment.equipment_id.has_value()) {
      suggested.equipment_ids.push_back(*slot_assignment.equipment_id);
    }
  }
  return suggested;
}

bool AssignmentsEquivalent(const Assignment& left, const Assignment& right) {
  return left.assignee_type == right.assignee_type && left.team_id == right.team_id &&
         left.individual_user_id == right.individual_user_id &&
         left.equipment_ids == right.equipment_ids &&
         left.member_user_ids == right.member_user_ids;
}

std::map<std::string, std::set<std::string>> CollectOrderCandidateResources(
    const std::vector<OrderInput>& orders, const std::string& resource_type) {
  std::map<std::string, std::set<std::string>> result;
  for (const auto& order : orders) {
    std::set<std::string> ids;
    if (resource_type == "employee") {
      for (const auto& slot : order.personnel_slots) {
        ids.insert(slot.candidate_user_ids.begin(), slot.candidate_user_ids.end());
      }
    } else {
      for (const auto& slot : order.equipment_slots) {
        ids.insert(slot.candidate_equipment_ids.begin(), slot.candidate_equipment_ids.end());
      }
    }
    result[order.order_id] = ids;
  }
  return result;
}

void ValidateModelCoverage(const std::vector<OrderInput>& orders,
                           const std::set<std::string>& fixed_anchor_order_ids,
                           const WindowLookup& window_lookup,
                           const TravelLookup& travel_lookup) {
  const auto employee_candidates = CollectOrderCandidateResources(orders, "employee");
  const auto equipment_candidates = CollectOrderCandidateResources(orders, "equipment");

  auto validate_resource_set = [&](const std::string& resource_type,
                                   const std::map<std::string, std::set<std::string>>& candidate_map) {
    std::set<std::string> resource_ids;
    for (const auto& entry : candidate_map) {
      resource_ids.insert(entry.second.begin(), entry.second.end());
    }
    for (const auto& resource_id : resource_ids) {
      const auto windows_it = window_lookup.find({resource_type, resource_id});
      if (windows_it == window_lookup.end() || windows_it->second.empty()) {
        throw std::runtime_error("invalid model: missing explicit free windows for " + resource_type +
                                 ":" + resource_id);
      }
    }

    for (const auto& order : orders) {
      const auto order_it = candidate_map.find(order.order_id);
      if (order_it == candidate_map.end()) {
        continue;
      }
      for (const auto& resource_id : order_it->second) {
        const auto windows_it = window_lookup.find({resource_type, resource_id});
        if (windows_it == window_lookup.end()) {
          throw std::runtime_error("invalid model: missing explicit free windows for " + resource_type +
                                   ":" + resource_id);
        }
        const std::string order_node = OrderNode(order.order_id);
        const std::string anchor_node = AnchorNode(resource_type, resource_id);
        for (const auto& window : windows_it->second) {
          if (window.left_anchor_order_id.has_value() &&
              fixed_anchor_order_ids.find(*window.left_anchor_order_id) ==
                  fixed_anchor_order_ids.end()) {
            throw std::runtime_error("invalid model: unknown left anchor order " +
                                     *window.left_anchor_order_id);
          }
          if (window.right_anchor_order_id.has_value() &&
              fixed_anchor_order_ids.find(*window.right_anchor_order_id) ==
                  fixed_anchor_order_ids.end()) {
            throw std::runtime_error("invalid model: unknown right anchor order " +
                                     *window.right_anchor_order_id);
          }
          if (window.left_anchor_order_id.has_value() || window.left_anchor_stand_id.has_value()) {
            RequireTravelMinutes(travel_lookup, resource_type, resource_id, anchor_node, order_node,
                                 "anchor to order travel");
          }
          if (window.right_anchor_order_id.has_value() || window.right_anchor_stand_id.has_value()) {
            RequireTravelMinutes(travel_lookup, resource_type, resource_id, order_node, anchor_node,
                                 "order to anchor travel");
          }
        }
      }
    }

    for (size_t left_index = 0; left_index < orders.size(); ++left_index) {
      for (size_t right_index = left_index + 1; right_index < orders.size(); ++right_index) {
        const auto& left_order = orders[left_index];
        const auto& right_order = orders[right_index];
        const auto& left_resources = candidate_map.at(left_order.order_id);
        const auto& right_resources = candidate_map.at(right_order.order_id);
        for (const auto& resource_id : left_resources) {
          if (right_resources.find(resource_id) == right_resources.end()) {
            continue;
          }
          RequireTravelMinutes(travel_lookup, resource_type, resource_id, OrderNode(left_order.order_id),
                               OrderNode(right_order.order_id), "order to order travel");
          RequireTravelMinutes(travel_lookup, resource_type, resource_id, OrderNode(right_order.order_id),
                               OrderNode(left_order.order_id), "order to order travel");
        }
      }
    }
  };

  validate_resource_set("employee", employee_candidates);
  validate_resource_set("equipment", equipment_candidates);
}

StageSolveResult SolveStage(CpModelBuilder* model_builder,
                            const LinearExpr& objective,
                            bool maximize,
                            int64_t budget_ms,
                            const std::string& stage_name) {
  if (maximize) {
    model_builder->Maximize(objective);
  } else {
    model_builder->Minimize(objective);
  }

  SatParameters parameters;
  parameters.set_max_time_in_seconds(
      std::max(0.001, static_cast<double>(budget_ms) / 1000.0));
  // The emscripten build links without -pthread (see tools/ortools_wasm/
  // CMakeLists.txt), so a worker portfolio is unavailable in the browser.
  parameters.set_num_search_workers(1);

  Model model;
  model.Add(NewSatParameters(parameters));
  const CpSolverResponse response = SolveCpModel(model_builder->Build(), &model);
  const std::string status_string = StatusToString(response.status());

  StageSolveResult result;
  result.response = response;
  result.objective_value =
      (response.status() == CpSolverStatus::OPTIMAL || response.status() == CpSolverStatus::FEASIBLE)
          ? SolutionIntegerValue(response, objective)
          : 0;
  result.record.stage = stage_name;
  result.record.solve_status = status_string;
  result.record.objective_value = result.objective_value;
  result.record.wall_time_ms = static_cast<int64_t>(std::llround(response.wall_time() * 1000.0));
  result.record.conflicts = response.num_conflicts();
  result.record.branches = response.num_branches();
  result.record.best_bound = response.best_objective_bound();
  // CP-SAT reports FEASIBLE (not UNKNOWN) when it hits the time limit holding
  // an incumbent it could not prove optimal. Treating that as success silently
  // freezes a suboptimal bound into every later stage, so record it explicitly.
  result.record.proven_optimal = response.status() == CpSolverStatus::OPTIMAL;
  result.record.budget_ms = budget_ms;
  return result;
}

std::optional<PersonnelSlotDecision*> FindPersonnelSlotDecision(OrderDecision* decision,
                                                                const std::string& slot_code) {
  for (auto& slot : decision->personnel_slots) {
    if (slot.slot.slot_code == slot_code) {
      return &slot;
    }
  }
  return std::nullopt;
}

std::optional<ResourceUsageDecision*> FindResourceUsage(OrderDecision* decision,
                                                        const std::string& resource_type,
                                                        const std::string& resource_id) {
  auto iterator = decision->resource_usages.find({resource_type, resource_id});
  if (iterator == decision->resource_usages.end()) {
    return std::nullopt;
  }
  return &iterator->second;
}

std::optional<TurnaroundEndpoint> FindTurnaroundEndpoint(
    std::vector<OrderDecision>* decisions,
    const std::map<std::string, size_t>& order_index_by_id,
    const std::map<std::string, const OrderInput*>& fixed_orders_by_id,
    const std::string& order_id,
    const std::string& slot_code) {
  const auto order_it = order_index_by_id.find(order_id);
  if (order_it != order_index_by_id.end()) {
    OrderDecision* decision = &(*decisions)[order_it->second];
    auto slot_opt = FindPersonnelSlotDecision(decision, slot_code);
    if (!slot_opt.has_value()) {
      return std::nullopt;
    }
    TurnaroundEndpoint endpoint;
    endpoint.order = &decision->order;
    endpoint.optimizable_slot = *slot_opt;
    endpoint.is_fixed = false;
    return endpoint;
  }

  const auto fixed_it = fixed_orders_by_id.find(order_id);
  if (fixed_it == fixed_orders_by_id.end()) {
    return std::nullopt;
  }
  TurnaroundEndpoint endpoint;
  endpoint.order = fixed_it->second;
  endpoint.fixed_assignment = FindBaselinePersonnelSlotAssignment(*fixed_it->second, slot_code);
  endpoint.is_fixed = true;
  return endpoint;
}

BoolVar BuildTurnaroundNonGapLiteral(CpModelBuilder* model_builder,
                                     const TurnaroundPair& pair,
                                     const std::string& side_label,
                                     const TurnaroundEndpoint& endpoint) {
  if (endpoint.is_fixed) {
    return FixedBool(model_builder,
                     endpoint.fixed_assignment.has_value() &&
                         endpoint.fixed_assignment->user_id.has_value(),
                     pair.pair_key + "_" + side_label + "_fixed_non_gap");
  }

  const BoolVar non_gap =
      model_builder->NewBoolVar().WithName(pair.pair_key + "_" + side_label + "_non_gap");
  model_builder->AddEquality(non_gap + endpoint.optimizable_slot->gap, 1);
  return non_gap;
}

std::optional<std::string> EvaluateSelectedCandidate(const CpSolverResponse& response,
                                                     const PersonnelSlotDecision& decision) {
  for (const auto& candidate : decision.candidates) {
    if (SolutionBooleanValue(response, candidate.selected)) {
      return candidate.candidate_id;
    }
  }
  return std::nullopt;
}

std::optional<std::string> EvaluateSelectedCandidate(const CpSolverResponse& response,
                                                     const EquipmentSlotDecision& decision) {
  for (const auto& candidate : decision.candidates) {
    if (SolutionBooleanValue(response, candidate.selected)) {
      return candidate.candidate_id;
    }
  }
  return std::nullopt;
}

val OptionalStringToVal(const std::optional<std::string>& value) {
  return value.has_value() ? val(*value) : val::null();
}

val ToCrewMemberVal(const CrewMember& member) {
  val output = val::object();
  output.set("user_id", OptionalStringToVal(member.user_id));
  output.set("username", OptionalStringToVal(member.username));
  output.set("source_team_id", OptionalStringToVal(member.source_team_id));
  output.set("source_team_name", OptionalStringToVal(member.source_team_name));
  output.set("slot_code", OptionalStringToVal(member.slot_code));
  output.set("qualification_code", OptionalStringToVal(member.qualification_code));
  output.set("qualification_level_code", OptionalStringToVal(member.qualification_level_code));
  return output;
}

val ToAssignmentVal(const Assignment& assignment) {
  val output = val::object();
  output.set("assignee_type", OptionalStringToVal(assignment.assignee_type));
  output.set("team_id", OptionalStringToVal(assignment.team_id));
  output.set("individual_user_id", OptionalStringToVal(assignment.individual_user_id));
  output.set("department_rule_version", OptionalStringToVal(assignment.department_rule_version));

  val equipment_ids = val::array();
  for (const auto& equipment_id : assignment.equipment_ids) {
    equipment_ids.call<void>("push", val(equipment_id));
  }
  output.set("equipment_ids", equipment_ids);

  val member_user_ids = val::array();
  for (const auto& user_id : assignment.member_user_ids) {
    member_user_ids.call<void>("push", val(user_id));
  }
  output.set("member_user_ids", member_user_ids);

  output.set("crew_requirement_snapshot", JsonParse(assignment.crew_requirement_snapshot_json));
  output.set("equipment_requirement_snapshot",
             JsonParse(assignment.equipment_requirement_snapshot_json));
  output.set("qualification_gap", JsonParse(assignment.qualification_gap_json));

  val task_crew = val::object();
  val members = val::array();
  for (const auto& member : assignment.task_crew_members) {
    members.call<void>("push", ToCrewMemberVal(member));
  }
  task_crew.set("members", members);
  val source_team_ids = val::array();
  for (const auto& source_team_id : assignment.task_crew_source_team_ids) {
    source_team_ids.call<void>("push", val(source_team_id));
  }
  task_crew.set("source_team_ids", source_team_ids);
  val source_team_names = val::array();
  for (const auto& source_team_name : assignment.task_crew_source_team_names) {
    source_team_names.call<void>("push", val(source_team_name));
  }
  task_crew.set("source_team_names", source_team_names);
  task_crew.set("generated_from", OptionalStringToVal(assignment.task_crew_generated_from));
  output.set("task_crew", task_crew);
  return output;
}

val BuildMemberChangeSummaryVal(const Assignment& current_assignment,
                                const Assignment& suggested_assignment) {
  std::map<std::string, CrewMember> current_by_slot;
  std::map<std::string, CrewMember> suggested_by_slot;
  for (const auto& member : current_assignment.task_crew_members) {
    const std::string key = member.slot_code.value_or(member.user_id.value_or(""));
    if (!key.empty()) {
      current_by_slot[key] = member;
    }
  }
  for (const auto& member : suggested_assignment.task_crew_members) {
    const std::string key = member.slot_code.value_or(member.user_id.value_or(""));
    if (!key.empty()) {
      suggested_by_slot[key] = member;
    }
  }

  std::set<std::string> keys;
  for (const auto& item : current_by_slot) {
    keys.insert(item.first);
  }
  for (const auto& item : suggested_by_slot) {
    keys.insert(item.first);
  }

  val output = val::object();
  val replaced_members = val::array();
  val added_members = val::array();
  val removed_members = val::array();
  val unchanged_members = val::array();
  int64_t changed_member_count = 0;
  for (const auto& key : keys) {
    const auto current_it = current_by_slot.find(key);
    const auto suggested_it = suggested_by_slot.find(key);
    const bool has_current = current_it != current_by_slot.end();
    const bool has_suggested = suggested_it != suggested_by_slot.end();
    if (has_current && has_suggested) {
      if (current_it->second.user_id == suggested_it->second.user_id) {
        val item = val::object();
        item.set("slot_code", val(key));
        item.set("member", ToCrewMemberVal(suggested_it->second));
        unchanged_members.call<void>("push", item);
      } else {
        val item = val::object();
        item.set("slot_code", val(key));
        item.set("before", ToCrewMemberVal(current_it->second));
        item.set("after", ToCrewMemberVal(suggested_it->second));
        replaced_members.call<void>("push", item);
        ++changed_member_count;
      }
    } else if (has_suggested) {
      val item = val::object();
      item.set("slot_code", val(key));
      item.set("member", ToCrewMemberVal(suggested_it->second));
      added_members.call<void>("push", item);
      ++changed_member_count;
    } else if (has_current) {
      val item = val::object();
      item.set("slot_code", val(key));
      item.set("member", ToCrewMemberVal(current_it->second));
      removed_members.call<void>("push", item);
      ++changed_member_count;
    }
  }

  output.set("replaced_members", replaced_members);
  output.set("added_members", added_members);
  output.set("removed_members", removed_members);
  output.set("unchanged_members", unchanged_members);
  output.set("changed_member_count", ToJsNumber(changed_member_count));
  return output;
}

SolveArtifacts SolveRequest(const std::string& input_json) {
  const val request = JsonParse(input_json);
  SolveArtifacts artifacts;
  artifacts.cluster_id = GetOptionalString(request, "cluster_id").value_or("dispatch-cluster");
  artifacts.model_version = GetOptionalString(request, "model_version").value_or(
      "dispatch_wasm_pdf_full_model_v2");
  artifacts.solver_version = GetOptionalString(request, "solver_version").value_or(kSolverVersion);

  std::vector<OrderInput> orders;
  for (const val& order_val : ToValVector(request["optimizable_orders"])) {
    orders.push_back(ParseOrderInput(order_val));
  }
  std::vector<OrderInput> fixed_anchor_orders;
  for (const val& order_val : ToValVector(request["fixed_anchor_orders"])) {
    fixed_anchor_orders.push_back(ParseOrderInput(order_val));
  }
  std::sort(orders.begin(), orders.end(), [](const OrderInput& left, const OrderInput& right) {
    return std::tie(left.original_start_min, left.order_id) <
           std::tie(right.original_start_min, right.order_id);
  });

  std::vector<ResourceWindow> windows;
  std::set<WindowIdentity> seen_windows;
  for (const val& item : ToValVector(request["employee_free_windows"])) {
    PushUniqueResourceWindow(&windows, &seen_windows, ParseResourceWindow(item));
  }
  for (const val& item : ToValVector(request["equipment_free_windows"])) {
    PushUniqueResourceWindow(&windows, &seen_windows, ParseResourceWindow(item));
  }
  AppendAnchorStateWindows(request["employee_anchor_states"], &windows, &seen_windows);
  AppendAnchorStateWindows(request["equipment_anchor_states"], &windows, &seen_windows);
  const WindowLookup window_lookup = BuildWindowLookup(windows);

  std::vector<TravelEdge> travel_edges;
  for (const val& item : ToValVector(request["resource_travel_edges"])) {
    travel_edges.push_back(ParseTravelEdge(item));
  }
  const TravelLookup travel_lookup = BuildTravelLookup(travel_edges);

  std::vector<TurnaroundPair> turnaround_pairs;
  for (const val& item : ToValVector(request["turnaround_pairs"])) {
    turnaround_pairs.push_back(ParseTurnaroundPair(item));
  }

  const int64_t timeout_ms = std::max<int64_t>(
      1, GetInt64(request["objective_config"], "timeout_ms", kDefaultTimeoutMs));
  const int64_t average_workload_target = std::max<int64_t>(
      0, GetInt64(request["objective_config"], "average_workload_target", 0));
  if (orders.empty() && fixed_anchor_orders.empty()) {
    return artifacts;
  }
  std::set<std::string> fixed_anchor_order_ids;
  for (const auto& order : fixed_anchor_orders) {
    fixed_anchor_order_ids.insert(order.order_id);
  }
  ValidateModelCoverage(orders, fixed_anchor_order_ids, window_lookup, travel_lookup);

  int64_t horizon_start = 0;
  int64_t horizon_end = 24 * 60;
  bool horizon_initialized = false;
  auto extend_horizon = [&](int64_t start_min, int64_t end_min) {
    if (!horizon_initialized) {
      horizon_start = start_min;
      horizon_end = end_min;
      horizon_initialized = true;
      return;
    }
    horizon_start = std::min(horizon_start, start_min);
    horizon_end = std::max(horizon_end, end_min);
  };
  // A crew-size table can make an order run longer than its input end time, so
  // the horizon has to cover the longest row rather than the nominal duration.
  auto horizon_end_for = [](const OrderInput& order) {
    const int64_t longest = order.duration_by_crew_size.empty()
                                ? order.duration_min
                                : *std::max_element(order.duration_by_crew_size.begin(),
                                                    order.duration_by_crew_size.end());
    return std::max(order.original_end_min,
                    std::max(order.latest_start_min, order.original_start_min) + longest);
  };
  for (const auto& order : orders) {
    extend_horizon(std::min(order.earliest_start_min, order.original_start_min),
                   horizon_end_for(order) + 12 * 60);
  }
  for (const auto& order : fixed_anchor_orders) {
    extend_horizon(std::min(order.earliest_start_min, order.original_start_min),
                   horizon_end_for(order) + 12 * 60);
  }
  for (const auto& entry : window_lookup) {
    for (const auto& window : entry.second) {
      extend_horizon(window.window_start_min, window.window_end_min + 12 * 60);
    }
  }
  if (!horizon_initialized) {
    horizon_start = 0;
    horizon_end = 24 * 60;
  }
  if (horizon_end < horizon_start + kMinimumOrderDurationMinutes) {
    horizon_end = horizon_start + 24 * 60;
  }

  CpModelBuilder model_builder;
  LinearExpr slot_gap_expression;
  LinearExpr lateness_expression;
  LinearExpr continuity_penalty_expression;
  LinearExpr baseline_change_expression;
  LinearExpr travel_cost_expression;
  LinearExpr scarcity_cost_expression;
  LinearExpr load_deviation_expression;

  std::vector<OrderDecision> decisions;
  decisions.reserve(orders.size());
  std::map<std::string, size_t> order_index_by_id;
  std::map<std::string, std::vector<BoolVar>> resource_usage_counts_by_key;
  std::map<std::string, OrderInput> order_lookup;
  for (const auto& order : orders) {
    order_lookup[order.order_id] = order;
  }

  for (const auto& order : orders) {
    OrderDecision decision;
    decision.order = order;
    decision.start = model_builder.NewIntVar(Domain(horizon_start, horizon_end))
                         .WithName(order.order_id + "_start");
    decision.lateness = model_builder.NewIntVar(Domain(0, horizon_end - horizon_start))
                            .WithName(order.order_id + "_lateness");
    decision.time_changed =
        model_builder.NewBoolVar().WithName(order.order_id + "_time_changed");

    // Planned times are forecasts, not SLA commitments. Keep the legacy result
    // field at zero for wire compatibility without optimizing artificial
    // lateness against planned_end_time.
    model_builder.AddEquality(decision.lateness, 0);
    lateness_expression += decision.lateness;
    model_builder.AddGreaterOrEqual(decision.start, order.earliest_start_min);
    model_builder.AddLessOrEqual(decision.start, order.latest_start_min);
    model_builder.AddEquality(decision.start, order.original_start_min)
        .OnlyEnforceIf(~decision.time_changed);
    model_builder.AddNotEqual(decision.start, order.original_start_min)
        .OnlyEnforceIf(decision.time_changed);

    std::map<std::string, std::vector<BoolVar>> order_personnel_candidate_literals;
    // Personnel gaps only. Equipment gaps go into the shared slot_gap objective
    // too, but the crew-size table is about how many people are on the job.
    LinearExpr personnel_gap_sum;
    for (const auto& slot : order.personnel_slots) {
      PersonnelSlotDecision slot_decision;
      slot_decision.slot = slot;
      slot_decision.gap =
          model_builder.NewBoolVar().WithName(order.order_id + "_" + slot.slot_code + "_gap");
      std::vector<BoolVar> state_literals;
      for (size_t candidate_index = 0; candidate_index < slot.candidate_user_ids.size();
           ++candidate_index) {
        const std::string& candidate_id = slot.candidate_user_ids[candidate_index];
        const BoolVar selected = model_builder.NewBoolVar().WithName(
            order.order_id + "_" + slot.slot_code + "_employee_" +
            std::to_string(candidate_index));
        slot_decision.candidates.push_back({candidate_id, selected});
        state_literals.push_back(selected);
        order_personnel_candidate_literals[candidate_id].push_back(selected);
        scarcity_cost_expression += selected * slot.scarcity_cost;
        if (slot.baseline_user_id.has_value() &&
            slot.baseline_user_id != std::optional<std::string>(candidate_id)) {
          baseline_change_expression += selected;
        }
      }
      state_literals.push_back(slot_decision.gap);
      // Exactly one of {each candidate, gap}. AddExactlyOne is a dedicated
      // constraint CP-SAT presolves and propagates directly, rather than one it
      // has to recover from a generic linear sum.
      model_builder.AddExactlyOne(state_literals);
      slot_gap_expression += slot_decision.gap;
      personnel_gap_sum += slot_decision.gap;
      decision.personnel_slots.push_back(slot_decision);
    }
    for (const auto& entry : order_personnel_candidate_literals) {
      // One employee cannot fill two slots of the same order.
      model_builder.AddAtMostOne(entry.second);
    }

    std::map<std::string, std::vector<BoolVar>> order_equipment_candidate_literals;
    for (const auto& slot : order.equipment_slots) {
      EquipmentSlotDecision slot_decision;
      slot_decision.slot = slot;
      slot_decision.gap =
          model_builder.NewBoolVar().WithName(order.order_id + "_" + slot.slot_code + "_gap");
      std::vector<BoolVar> state_literals;
      for (size_t candidate_index = 0; candidate_index < slot.candidate_equipment_ids.size();
           ++candidate_index) {
        const std::string& candidate_id = slot.candidate_equipment_ids[candidate_index];
        const BoolVar selected = model_builder.NewBoolVar().WithName(
            order.order_id + "_" + slot.slot_code + "_equipment_" +
            std::to_string(candidate_index));
        slot_decision.candidates.push_back({candidate_id, selected});
        state_literals.push_back(selected);
        order_equipment_candidate_literals[candidate_id].push_back(selected);
      }
      state_literals.push_back(slot_decision.gap);
      model_builder.AddExactlyOne(state_literals);
      slot_gap_expression += slot_decision.gap;
      decision.equipment_slots.push_back(slot_decision);
    }
    for (const auto& entry : order_equipment_candidate_literals) {
      // One unit of equipment cannot fill two slots of the same order.
      model_builder.AddAtMostOne(entry.second);
    }

    // Duration: a constant, unless the department published a crew-size table.
    //
    // With a table, duration becomes a decision variable indexed by how many
    // personnel slots are filled, so a short-staffed job stretches and the
    // stretch propagates into every downstream occupancy and travel bound. The
    // channelling is one-hot rather than AddElement so it uses only constructs
    // already proven to compile against this OR-Tools build.
    //
    // This does not create an incentive to under-staff: slot_gap is the first
    // lexicographic stage, so filling slots is settled before duration is ever
    // weighed. The table matters exactly when slots genuinely cannot be filled.
    if (order.duration_by_crew_size.empty()) {
      decision.duration = LinearExpr(order.duration_min);
      decision.end = decision.start + order.duration_min;
    } else {
      const auto& table = order.duration_by_crew_size;
      const auto bounds = std::minmax_element(table.begin(), table.end());
      IntVar duration_var =
          model_builder.NewIntVar(Domain(*bounds.first, *bounds.second))
              .WithName(order.order_id + "_duration");
      std::vector<BoolVar> crew_size_literals;
      crew_size_literals.reserve(table.size());
      for (size_t crew_size = 0; crew_size < table.size(); ++crew_size) {
        const BoolVar is_crew_size = model_builder.NewBoolVar().WithName(
            order.order_id + "_crew_size_" + std::to_string(crew_size));
        crew_size_literals.push_back(is_crew_size);
        model_builder
            .AddEquality(LinearExpr(static_cast<int64_t>(order.personnel_slots.size())) -
                             personnel_gap_sum,
                         static_cast<int64_t>(crew_size))
            .OnlyEnforceIf(is_crew_size);
        model_builder.AddEquality(duration_var, table[crew_size])
            .OnlyEnforceIf(is_crew_size);
      }
      // Exactly one literal plus the forward implications above is a complete
      // channelling: the filled count picks its literal, which picks the row.
      model_builder.AddExactlyOne(crew_size_literals);
      decision.duration = LinearExpr(duration_var);
      // A dedicated end variable, tied to start + duration by a plain linear
      // equality. The interval below needs an affine end, and `start +
      // duration_var` is not affine once duration is a variable.
      // Deliberately wider than the horizon by one full table row: `start`
      // itself ranges up to horizon_end, so a tighter bound here would quietly
      // forbid late starts instead of merely bounding the end.
      IntVar end_var =
          model_builder.NewIntVar(Domain(horizon_start, horizon_end + *bounds.second))
              .WithName(order.order_id + "_end");
      model_builder.AddEquality(end_var, decision.start + duration_var);
      decision.end = LinearExpr(end_var);
    }
    if (order.has_fixed_completion_target) {
      model_builder.AddEquality(decision.end, order.completion_target_min);
    }

    for (auto& slot_decision : decision.personnel_slots) {
      for (const auto& candidate : slot_decision.candidates) {
        auto& usage = decision.resource_usages[{"employee", candidate.candidate_id}];
        usage.resource_type = "employee";
        usage.resource_id = candidate.candidate_id;
        usage.selectors.push_back(candidate.selected);
      }
    }
    for (auto& slot_decision : decision.equipment_slots) {
      for (const auto& candidate : slot_decision.candidates) {
        auto& usage = decision.resource_usages[{"equipment", candidate.candidate_id}];
        usage.resource_type = "equipment";
        usage.resource_id = candidate.candidate_id;
        usage.selectors.push_back(candidate.selected);
      }
    }

    for (auto& entry : decision.resource_usages) {
      auto& usage = entry.second;
      usage.used = model_builder.NewBoolVar().WithName(
          order.order_id + "_" + usage.resource_type + "_" + usage.resource_id + "_used");
      if (usage.selectors.empty()) {
        model_builder.FixVariable(usage.used, false);
      } else {
        model_builder.AddEquality(usage.used, SumBoolVars(usage.selectors));
      }

      // Optional interval for the disjunctive view of this resource. Shares the
      // order's start variable, so NoOverlap and the sequencing literals below
      // constrain the same decision rather than parallel copies of it.
      usage.occupancy = model_builder.NewOptionalIntervalVar(
          decision.start, decision.duration, decision.end, usage.used)
          .WithName(order.order_id + "_" + usage.resource_type + "_" + usage.resource_id +
                    "_occupancy");

      const auto windows_it = window_lookup.find({usage.resource_type, usage.resource_id});
      if (windows_it == window_lookup.end() || windows_it->second.empty()) {
        throw std::runtime_error("invalid model: missing explicit free windows for " +
                                 usage.resource_type + ":" + usage.resource_id);
      }
      std::vector<BoolVar> window_literals;
      for (size_t window_index = 0; window_index < windows_it->second.size(); ++window_index) {
        const ResourceWindow& window = windows_it->second[window_index];
        const BoolVar selected = model_builder.NewBoolVar().WithName(
            order.order_id + "_" + usage.resource_type + "_" + usage.resource_id + "_window_" +
            std::to_string(window_index));
        int64_t left_travel = 0;
        int64_t right_travel = 0;
        if (window.left_anchor_order_id.has_value() || window.left_anchor_stand_id.has_value()) {
          left_travel = RequireTravelMinutes(travel_lookup, usage.resource_type, usage.resource_id,
                                             AnchorNode(usage.resource_type, usage.resource_id),
                                             OrderNode(order.order_id), "anchor to order travel");
        }
        if (window.right_anchor_order_id.has_value() || window.right_anchor_stand_id.has_value()) {
          right_travel = RequireTravelMinutes(travel_lookup, usage.resource_type, usage.resource_id,
                                              OrderNode(order.order_id),
                                              AnchorNode(usage.resource_type, usage.resource_id),
                                              "order to anchor travel");
        }
        window_literals.push_back(selected);
        WindowChoiceDecision window_choice;
        window_choice.window = window;
        window_choice.selected = selected;
        window_choice.is_first = model_builder.NewBoolVar().WithName(
            order.order_id + "_" + usage.resource_type + "_" + usage.resource_id + "_window_" +
            std::to_string(window_index) + "_first");
        window_choice.is_last = model_builder.NewBoolVar().WithName(
            order.order_id + "_" + usage.resource_type + "_" + usage.resource_id + "_window_" +
            std::to_string(window_index) + "_last");
        window_choice.left_anchor_travel = left_travel;
        window_choice.right_anchor_travel = right_travel;
        usage.window_choices.push_back(window_choice);
      }
      model_builder.AddEquality(SumBoolVars(window_literals), usage.used);
      resource_usage_counts_by_key[usage.resource_type + ":" + usage.resource_id].push_back(usage.used);
    }

    order_index_by_id[order.order_id] = decisions.size();
    decisions.push_back(decision);
  }

  std::map<ResourceKey, std::vector<size_t>> decision_indices_by_resource;
  for (size_t decision_index = 0; decision_index < decisions.size(); ++decision_index) {
    for (const auto& usage_entry : decisions[decision_index].resource_usages) {
      decision_indices_by_resource[usage_entry.first].push_back(decision_index);
    }
  }

  // One disjunctive constraint per resource. This is redundant with the
  // per-window sequencing literals below -- every schedule they admit is
  // already non-overlapping, and free windows are cut around the fixed anchors
  // so those need no interval of their own -- but stating it directly gives
  // CP-SAT its disjunctive and timetable propagators, which prune start domains
  // long before the sequencing literals have been fixed.
  for (const auto& resource_entry : decision_indices_by_resource) {
    const auto& resource_key = resource_entry.first;
    std::vector<IntervalVar> occupancy_intervals;
    for (const size_t decision_index : resource_entry.second) {
      auto usage_opt =
          FindResourceUsage(&decisions[decision_index], resource_key.first, resource_key.second);
      if (usage_opt.has_value()) {
        occupancy_intervals.push_back((*usage_opt)->occupancy);
      }
    }
    if (occupancy_intervals.size() > 1) {
      model_builder.AddNoOverlap(occupancy_intervals);
    }
  }

  std::vector<ResourcePathEdgeDecision> resource_path_edges;
  for (const auto& resource_entry : decision_indices_by_resource) {
    const auto& resource_key = resource_entry.first;
    const auto& decision_indices = resource_entry.second;
    const auto windows_it = window_lookup.find(resource_key);
    if (windows_it == window_lookup.end()) {
      throw std::runtime_error("invalid model: missing free window lookup for " +
                               resource_key.first + ":" + resource_key.second);
    }
    for (size_t window_index = 0; window_index < windows_it->second.size(); ++window_index) {
      // One circuit per (resource, window). Node 0 is the window boundary --
      // the fixed anchor work cut out around this gap -- and each order using
      // this resource occupies node (local_index + 1). The circuit enforces a
      // Hamiltonian cycle on the selected orders (a total order, no subtours,
      // no repeated node), which is exactly the sequencing structure the MTZ
      // rank variables used to encode. It is an order on the nodes, not on
      // absolute time; the travel bounds below turn the cycle into a schedule.
      CircuitConstraint circuit = model_builder.AddCircuitConstraint();
      std::vector<std::vector<BoolVar>> incoming_literals(decision_indices.size());
      std::vector<std::vector<BoolVar>> outgoing_literals(decision_indices.size());
      std::vector<BoolVar> first_literals;
      std::vector<BoolVar> last_literals;

      for (size_t local_index = 0; local_index < decision_indices.size(); ++local_index) {
        OrderDecision& decision = decisions[decision_indices[local_index]];
        auto usage_opt = FindResourceUsage(&decision, resource_key.first, resource_key.second);
        if (!usage_opt.has_value()) {
          throw std::runtime_error("invalid model: missing resource usage for " + decision.order.order_id);
        }
        auto& usage = **usage_opt;
        auto& window_choice = usage.window_choices[window_index];
        model_builder.AddLessOrEqual(window_choice.is_first, window_choice.selected);
        model_builder.AddLessOrEqual(window_choice.is_last, window_choice.selected);
        model_builder
            .AddGreaterOrEqual(decision.start,
                               window_choice.window.window_start_min +
                                   window_choice.left_anchor_travel)
            .OnlyEnforceIf(window_choice.is_first);
        model_builder
            .AddLessOrEqual(decision.end + window_choice.right_anchor_travel,
                            window_choice.window.window_end_min)
            .OnlyEnforceIf(window_choice.is_last);
        travel_cost_expression += window_choice.is_first * window_choice.left_anchor_travel;
        travel_cost_expression += window_choice.is_last * window_choice.right_anchor_travel;
        first_literals.push_back(window_choice.is_first);
        last_literals.push_back(window_choice.is_last);
      }

      // AddCircuit treats a node carrying a true self-loop as excluded from the
      // circuit, so an all-self-loop assignment is the empty circuit. This
      // literal is what lets an unused window reach that state: it is true iff
      // the resource visits at least one order here, which is equivalent to
      // having exactly one first and one last order.
      const BoolVar window_active = model_builder.NewBoolVar().WithName(
          resource_key.first + "_" + resource_key.second + "_window_" +
          std::to_string(window_index) + "_active");
      model_builder.AddEquality(SumBoolVars(first_literals), window_active);
      model_builder.AddEquality(SumBoolVars(last_literals), window_active);
      circuit.AddArc(0, 0, window_active.Not());
      for (size_t from_local_index = 0; from_local_index < decision_indices.size();
           ++from_local_index) {
        OrderDecision& from_decision = decisions[decision_indices[from_local_index]];
        auto from_usage_opt =
            FindResourceUsage(&from_decision, resource_key.first, resource_key.second);
        auto& from_usage = **from_usage_opt;
        auto& from_window_choice = from_usage.window_choices[window_index];

        for (size_t to_local_index = 0; to_local_index < decision_indices.size(); ++to_local_index) {
          if (from_local_index == to_local_index) {
            continue;
          }
          OrderDecision& to_decision = decisions[decision_indices[to_local_index]];
          auto to_usage_opt = FindResourceUsage(&to_decision, resource_key.first, resource_key.second);
          auto& to_usage = **to_usage_opt;
          auto& to_window_choice = to_usage.window_choices[window_index];
          const int64_t travel_minutes = RequireTravelMinutes(
              travel_lookup, resource_key.first, resource_key.second,
              OrderNode(from_decision.order.order_id), OrderNode(to_decision.order.order_id),
              "order to order travel");

          ResourcePathEdgeDecision edge;
          edge.resource_type = resource_key.first;
          edge.resource_id = resource_key.second;
          edge.window_index = window_index;
          edge.from_order_id = from_decision.order.order_id;
          edge.to_order_id = to_decision.order.order_id;
          edge.selected = model_builder.NewBoolVar().WithName(
              resource_key.first + "_" + resource_key.second + "_window_" +
              std::to_string(window_index) + "_" + from_decision.order.order_id + "_" +
              to_decision.order.order_id + "_edge");
          edge.travel_minutes = travel_minutes;
          model_builder.AddLessOrEqual(edge.selected, from_window_choice.selected);
          model_builder.AddLessOrEqual(edge.selected, to_window_choice.selected);
          model_builder
              .AddGreaterOrEqual(to_decision.start,
                                 from_decision.end + travel_minutes)
              .OnlyEnforceIf(edge.selected);
          outgoing_literals[from_local_index].push_back(edge.selected);
          incoming_literals[to_local_index].push_back(edge.selected);
          travel_cost_expression += edge.selected * travel_minutes;
          resource_path_edges.push_back(edge);
          circuit.AddArc(static_cast<int>(from_local_index) + 1,
                         static_cast<int>(to_local_index) + 1, edge.selected);
        }
        circuit.AddArc(static_cast<int>(from_local_index) + 1, 0, from_window_choice.is_last);
        circuit.AddArc(0, static_cast<int>(from_local_index) + 1, from_window_choice.is_first);
        circuit.AddArc(static_cast<int>(from_local_index) + 1,
                       static_cast<int>(from_local_index) + 1,
                       from_window_choice.selected.Not());
      }

      for (size_t local_index = 0; local_index < decision_indices.size(); ++local_index) {
        OrderDecision& decision = decisions[decision_indices[local_index]];
        auto usage_opt = FindResourceUsage(&decision, resource_key.first, resource_key.second);
        auto& usage = **usage_opt;
        auto& window_choice = usage.window_choices[window_index];
        model_builder.AddEquality(window_choice.is_first + SumBoolVars(incoming_literals[local_index]),
                                  window_choice.selected);
        model_builder.AddEquality(window_choice.is_last + SumBoolVars(outgoing_literals[local_index]),
                                  window_choice.selected);
      }

      model_builder.AddLessOrEqual(SumBoolVars(first_literals), 1);
      model_builder.AddLessOrEqual(SumBoolVars(last_literals), 1);
    }
  }

  std::map<std::string, const OrderInput*> fixed_orders_by_id;
  for (const auto& order : fixed_anchor_orders) {
    fixed_orders_by_id[order.order_id] = &order;
  }

  std::vector<TurnaroundDecision> turnaround_decisions;
  for (const auto& pair : turnaround_pairs) {
    auto inbound_endpoint_opt = FindTurnaroundEndpoint(
        &decisions, order_index_by_id, fixed_orders_by_id, pair.inbound_order_id, pair.inbound_slot_code);
    auto outbound_endpoint_opt = FindTurnaroundEndpoint(
        &decisions, order_index_by_id, fixed_orders_by_id, pair.outbound_order_id, pair.outbound_slot_code);
    if (!inbound_endpoint_opt.has_value() || !outbound_endpoint_opt.has_value()) {
      continue;
    }

    const TurnaroundEndpoint& inbound_endpoint = *inbound_endpoint_opt;
    const TurnaroundEndpoint& outbound_endpoint = *outbound_endpoint_opt;
    const BoolVar inbound_non_gap =
        BuildTurnaroundNonGapLiteral(&model_builder, pair, "inbound", inbound_endpoint);
    const BoolVar outbound_non_gap =
        BuildTurnaroundNonGapLiteral(&model_builder, pair, "outbound", outbound_endpoint);

    TurnaroundDecision decision;
    decision.pair = pair;
    decision.both_non_gap =
        model_builder.NewBoolVar().WithName(pair.pair_key + "_both_non_gap");
    decision.violation = pair.hard_continuity_required
                             ? FixedBool(&model_builder, false,
                                         pair.pair_key + "_hard_violation")
                             : model_builder.NewBoolVar().WithName(pair.pair_key + "_violation");

    model_builder.AddLessOrEqual(decision.both_non_gap, inbound_non_gap);
    model_builder.AddLessOrEqual(decision.both_non_gap, outbound_non_gap);
    model_builder.AddGreaterOrEqual(decision.both_non_gap, inbound_non_gap + outbound_non_gap - 1);

    if (!inbound_endpoint.is_fixed && !outbound_endpoint.is_fixed) {
      std::map<std::string, BoolVar> outbound_by_candidate;
      for (const auto& candidate : outbound_endpoint.optimizable_slot->candidates) {
        outbound_by_candidate[candidate.candidate_id] = candidate.selected;
      }
      for (const auto& inbound_candidate : inbound_endpoint.optimizable_slot->candidates) {
        const auto outbound_candidate_it =
            outbound_by_candidate.find(inbound_candidate.candidate_id);
        if (outbound_candidate_it == outbound_by_candidate.end()) {
          continue;
        }
        const BoolVar same_candidate = model_builder.NewBoolVar().WithName(
            pair.pair_key + "_" + inbound_candidate.candidate_id + "_same_candidate");
        model_builder.AddLessOrEqual(same_candidate, inbound_candidate.selected);
        model_builder.AddLessOrEqual(same_candidate, outbound_candidate_it->second);
        model_builder.AddGreaterOrEqual(
            same_candidate, inbound_candidate.selected + outbound_candidate_it->second - 1);
        decision.same_candidate_matches.push_back(same_candidate);
      }
    } else if (inbound_endpoint.is_fixed && outbound_endpoint.is_fixed) {
      const bool has_inbound_user = inbound_endpoint.fixed_assignment.has_value() &&
                                    inbound_endpoint.fixed_assignment->user_id.has_value();
      const bool has_outbound_user = outbound_endpoint.fixed_assignment.has_value() &&
                                     outbound_endpoint.fixed_assignment->user_id.has_value();
      const bool same_user = has_inbound_user && has_outbound_user &&
                             inbound_endpoint.fixed_assignment->user_id ==
                                 outbound_endpoint.fixed_assignment->user_id;
      if (pair.hard_continuity_required && has_inbound_user && has_outbound_user && !same_user) {
        artifacts.solve_status = "INFEASIBLE";
        artifacts.feasible = false;
        artifacts.error =
            "hard continuity conflict between fixed anchors for pair " + pair.pair_key;
        return artifacts;
      }
      decision.same_candidate_matches.push_back(
          FixedBool(&model_builder, same_user, pair.pair_key + "_fixed_same_candidate"));
    } else {
      const TurnaroundEndpoint& fixed_endpoint =
          inbound_endpoint.is_fixed ? inbound_endpoint : outbound_endpoint;
      const TurnaroundEndpoint& optimizable_endpoint =
          inbound_endpoint.is_fixed ? outbound_endpoint : inbound_endpoint;
      if (fixed_endpoint.fixed_assignment.has_value() &&
          fixed_endpoint.fixed_assignment->user_id.has_value()) {
        for (const auto& candidate : optimizable_endpoint.optimizable_slot->candidates) {
          if (candidate.candidate_id == *fixed_endpoint.fixed_assignment->user_id) {
            decision.same_candidate_matches.push_back(candidate.selected);
          }
        }
      }
    }

    const LinearExpr same_sum = SumBoolVars(decision.same_candidate_matches);
    if (pair.hard_continuity_required) {
      model_builder.AddGreaterOrEqual(same_sum, decision.both_non_gap);
    } else {
      model_builder.AddLessOrEqual(decision.violation, decision.both_non_gap);
      model_builder.AddGreaterOrEqual(decision.violation + same_sum, decision.both_non_gap);
      model_builder.AddLessOrEqual(decision.violation + same_sum, 1);
      continuity_penalty_expression +=
          decision.violation *
          std::max<int64_t>(pair.tightness_penalty, pair.continuity_penalty_weight);
    }
    turnaround_decisions.push_back(decision);
  }

  std::map<std::string, std::vector<LinearExpr>> weighted_load_terms_by_employee;
  for (const auto& decision : decisions) {
    for (const auto& slot_decision : decision.personnel_slots) {
      for (const auto& candidate : slot_decision.candidates) {
        weighted_load_terms_by_employee[candidate.candidate_id].push_back(
            candidate.selected * slot_decision.slot.workload_weight);
      }
    }
  }
  for (const auto& entry : weighted_load_terms_by_employee) {
    LinearExpr employee_load;
    for (const auto& term : entry.second) {
      employee_load += term;
    }
    const IntVar deviation = model_builder.NewIntVar(Domain(0, horizon_end - horizon_start + 10000))
                                 .WithName(entry.first + "_weighted_load_deviation");
    model_builder.AddAbsEquality(deviation, employee_load - average_workload_target);
    load_deviation_expression += deviation;
  }

  // Staged lexicographic refinement. `timeout_ms` is a budget for the whole
  // cluster solve, not per stage: stages that finish early donate their unused
  // time to later ones, so a caller asking for 10s can no longer wait 70s.
  constexpr int kStageCount = 6;
  int stages_completed = 0;
  int64_t budget_spent_ms = 0;
  auto next_stage_budget = [&]() -> int64_t {
    const int stages_left = std::max(1, kStageCount - stages_completed);
    const int64_t remaining = std::max<int64_t>(1, timeout_ms - budget_spent_ms);
    return std::max<int64_t>(1, remaining / stages_left);
  };

  // Carry the previous stage's solution forward as a hint. It is always
  // feasible for the next stage (which only adds a bound on an already-solved
  // objective), so it hands CP-SAT an immediate incumbent instead of making it
  // rediscover one from scratch on every one of the seven solves.
  auto hint_from = [&](const CpSolverResponse& response) {
    operations_research::sat::CpModelProto* proto = model_builder.MutableProto();
    proto->clear_solution_hint();
    const int hintable =
        std::min(proto->variables_size(), response.solution_size());
    if (hintable <= 0) {
      return;
    }
    auto* hint = proto->mutable_solution_hint();
    for (int index = 0; index < hintable; ++index) {
      hint->add_vars(index);
      hint->add_values(response.solution(index));
    }
  };

  std::vector<StageSolveResult> stage_results;
  stage_results.reserve(kStageCount);
  auto solve_and_lock = [&](const std::string& stage_name,
                            const LinearExpr& expression,
                            bool maximize) -> bool {
    StageSolveResult result =
        SolveStage(&model_builder, expression, maximize, next_stage_budget(), stage_name);
    stage_results.push_back(result);
    artifacts.stage_records.push_back(result.record);
    artifacts.wall_time_ms += result.record.wall_time_ms;
    budget_spent_ms += result.record.wall_time_ms;
    stages_completed += 1;
    artifacts.conflicts += result.record.conflicts;
    artifacts.branches += result.record.branches;
    artifacts.best_bound = result.record.best_bound;
    artifacts.timed_out = artifacts.timed_out || !result.record.proven_optimal;
    const bool stage_feasible =
        result.response.status() == CpSolverStatus::OPTIMAL ||
        result.response.status() == CpSolverStatus::FEASIBLE;
    if (!stage_feasible) {
      artifacts.solve_status = result.record.solve_status;
      artifacts.feasible = false;
      artifacts.error = "stage " + stage_name + " failed with status " +
                        result.record.solve_status;
      return false;
    }
    if (!result.record.proven_optimal) {
      // The locked value is an incumbent, not the stage optimum. Later stages
      // stay correct with respect to every hard constraint, but the objective
      // priority order below this stage is no longer guaranteed, so surface it
      // instead of reporting a clean OPTIMAL further down.
      artifacts.lexicographic_degraded = true;
      artifacts.degraded_stages.push_back(stage_name);
    }
    // Bound rather than pin. For a minimized stage the lexicographic refinement
    // is identical at the optimum, but an inequality cannot make a later stage
    // INFEASIBLE when this stage stopped on an incumbent, and it leaves the
    // relaxation free to improve the value instead of freezing it.
    if (maximize) {
      model_builder.AddGreaterOrEqual(expression, result.objective_value);
    } else {
      model_builder.AddLessOrEqual(expression, result.objective_value);
    }
    hint_from(result.response);
    return true;
  };

  if (!solve_and_lock("slot_gap", slot_gap_expression, false)) {
    return artifacts;
  }
  if (!solve_and_lock("continuity_break", continuity_penalty_expression, false)) {
    return artifacts;
  }
  if (!solve_and_lock("baseline_change", baseline_change_expression, false)) {
    return artifacts;
  }
  if (!solve_and_lock("travel_cost", travel_cost_expression, false)) {
    return artifacts;
  }
  if (!solve_and_lock("scarcity_cost", scarcity_cost_expression, false)) {
    return artifacts;
  }
  // Final stage takes the whole remaining budget rather than a fresh full
  // timeout, so the six stages together honour the caller's limit.
  const StageSolveResult final_stage =
      SolveStage(&model_builder, load_deviation_expression, false,
                 std::max<int64_t>(1, timeout_ms - budget_spent_ms), "load_deviation");
  stage_results.push_back(final_stage);
  artifacts.stage_records.push_back(final_stage.record);
  artifacts.wall_time_ms += final_stage.record.wall_time_ms;
  budget_spent_ms += final_stage.record.wall_time_ms;
  stages_completed += 1;
  artifacts.conflicts += final_stage.record.conflicts;
  artifacts.branches += final_stage.record.branches;
  artifacts.best_bound = final_stage.record.best_bound;
  artifacts.timed_out = artifacts.timed_out || !final_stage.record.proven_optimal;
  const bool final_stage_feasible =
      final_stage.response.status() == CpSolverStatus::OPTIMAL ||
      final_stage.response.status() == CpSolverStatus::FEASIBLE;
  if (!final_stage_feasible) {
    artifacts.solve_status = final_stage.record.solve_status;
    artifacts.feasible = false;
    artifacts.error = "stage load_deviation failed with status " +
                      final_stage.record.solve_status;
    return artifacts;
  }
  if (!final_stage.record.proven_optimal) {
    artifacts.lexicographic_degraded = true;
    artifacts.degraded_stages.push_back("load_deviation");
  }
  artifacts.solve_status = final_stage.record.solve_status;
  artifacts.feasible = final_stage_feasible;
  const CpSolverResponse& response = final_stage.response;

  artifacts.slot_gap = SolutionIntegerValue(response, slot_gap_expression);
  // slot_gap_expression sums the gap literals of both personnel and equipment
  // slots, so a zero there means nothing at all was left unfilled.
  artifacts.plan_complete = final_stage_feasible && artifacts.slot_gap == 0;
  artifacts.total_lateness_minutes = SolutionIntegerValue(response, lateness_expression);
  artifacts.continuity_penalty =
      SolutionIntegerValue(response, continuity_penalty_expression);
  artifacts.baseline_change = SolutionIntegerValue(response, baseline_change_expression);
  artifacts.travel_cost = SolutionIntegerValue(response, travel_cost_expression);
  artifacts.scarcity_cost = SolutionIntegerValue(response, scarcity_cost_expression);
  artifacts.load_deviation = SolutionIntegerValue(response, load_deviation_expression);

  std::map<std::string, std::vector<ContinuityDecisionResult>> continuity_by_order;
  for (const auto& decision : turnaround_decisions) {
    int64_t penalty_applied = 0;
    bool satisfied = true;
    if (decision.pair.hard_continuity_required) {
      bool both_non_gap = SolutionBooleanValue(response, decision.both_non_gap);
      bool has_match = false;
      for (const BoolVar literal : decision.same_candidate_matches) {
        has_match = has_match || SolutionBooleanValue(response, literal);
      }
      satisfied = !both_non_gap || has_match;
    } else {
      const bool violated = SolutionBooleanValue(response, decision.violation);
      penalty_applied =
          violated ? std::max<int64_t>(decision.pair.tightness_penalty,
                                       decision.pair.continuity_penalty_weight)
                   : 0;
      satisfied = !violated;
    }
    ContinuityDecisionResult result;
    result.pair_key = decision.pair.pair_key;
    result.inbound_order_id = decision.pair.inbound_order_id;
    result.outbound_order_id = decision.pair.outbound_order_id;
    result.inbound_slot_code = decision.pair.inbound_slot_code;
    result.outbound_slot_code = decision.pair.outbound_slot_code;
    result.satisfied = satisfied;
    result.hard_continuity_required = decision.pair.hard_continuity_required;
    result.penalty_applied = penalty_applied;
    artifacts.continuity_decisions.push_back(result);
    if (!satisfied) {
      ++artifacts.continuity_break;
    }
    continuity_by_order[result.inbound_order_id].push_back(result);
    continuity_by_order[result.outbound_order_id].push_back(result);
  }
  std::sort(artifacts.continuity_decisions.begin(), artifacts.continuity_decisions.end(),
            [](const ContinuityDecisionResult& left,
               const ContinuityDecisionResult& right) {
              return std::tie(left.pair_key, left.inbound_order_id, left.outbound_order_id) <
                     std::tie(right.pair_key, right.inbound_order_id, right.outbound_order_id);
            });

  std::map<std::string, int64_t> travel_minutes_by_order;
  for (const auto& decision : decisions) {
    travel_minutes_by_order[decision.order.order_id] = 0;
  }
  for (const auto& decision : decisions) {
    for (const auto& usage_entry : decision.resource_usages) {
      const auto& usage = usage_entry.second;
      for (const auto& window_choice : usage.window_choices) {
        if (!SolutionBooleanValue(response, window_choice.selected)) {
          continue;
        }
        if (SolutionBooleanValue(response, window_choice.is_first)) {
          travel_minutes_by_order[decision.order.order_id] +=
              window_choice.left_anchor_travel;
        }
        if (SolutionBooleanValue(response, window_choice.is_last)) {
          travel_minutes_by_order[decision.order.order_id] +=
              window_choice.right_anchor_travel;
        }
      }
    }
  }
  for (const auto& edge : resource_path_edges) {
    if (SolutionBooleanValue(response, edge.selected)) {
      travel_minutes_by_order[edge.from_order_id] += edge.travel_minutes;
    }
  }

  std::map<std::string, int64_t> scarcity_cost_by_order;
  std::map<std::string, int64_t> load_deviation_by_order;
  std::map<std::string, int64_t> selected_workload_by_employee;
  std::map<std::string, std::vector<std::pair<std::string, int64_t>>> workload_terms_by_employee;
  for (const auto& decision : decisions) {
    scarcity_cost_by_order[decision.order.order_id] = 0;
    load_deviation_by_order[decision.order.order_id] = 0;
    for (const auto& slot_decision : decision.personnel_slots) {
      for (const auto& candidate : slot_decision.candidates) {
        if (!SolutionBooleanValue(response, candidate.selected)) {
          continue;
        }
        scarcity_cost_by_order[decision.order.order_id] += slot_decision.slot.scarcity_cost;
        selected_workload_by_employee[candidate.candidate_id] +=
            slot_decision.slot.workload_weight;
        workload_terms_by_employee[candidate.candidate_id].push_back(
            {decision.order.order_id, slot_decision.slot.workload_weight});
      }
    }
  }
  for (const auto& entry : workload_terms_by_employee) {
    const auto total_load_it = selected_workload_by_employee.find(entry.first);
    const int64_t total_load =
        total_load_it == selected_workload_by_employee.end() ? 0 : total_load_it->second;
    if (total_load <= 0) {
      continue;
    }
    const int64_t deviation =
        total_load >= average_workload_target ? total_load - average_workload_target
                                              : average_workload_target - total_load;
    if (deviation <= 0) {
      continue;
    }
    struct Allocation {
      std::string order_id;
      int64_t base = 0;
      int64_t remainder = 0;
    };
    std::vector<Allocation> allocations;
    allocations.reserve(entry.second.size());
    int64_t assigned = 0;
    for (const auto& term : entry.second) {
      const int64_t numerator = deviation * term.second;
      const int64_t base = numerator / total_load;
      const int64_t remainder = numerator % total_load;
      allocations.push_back({term.first, base, remainder});
      assigned += base;
    }
    std::sort(allocations.begin(), allocations.end(),
              [](const Allocation& left, const Allocation& right) {
                if (left.remainder != right.remainder) {
                  return left.remainder > right.remainder;
                }
                return left.order_id < right.order_id;
              });
    int64_t remaining = deviation - assigned;
    for (size_t index = 0; index < allocations.size() && remaining > 0; ++index, --remaining) {
      ++allocations[index].base;
    }
    for (const auto& allocation : allocations) {
      load_deviation_by_order[allocation.order_id] += allocation.base;
    }
  }

  for (const auto& decision : decisions) {
    std::vector<PersonnelSlotAssignmentResult> order_personnel_results;
    std::vector<EquipmentSlotAssignmentResult> order_equipment_results;

    for (const auto& slot_decision : decision.personnel_slots) {
      const auto selected_user_id =
          EvaluateSelectedCandidate(response, slot_decision);
      const auto baseline =
          FindBaselinePersonnelSlotAssignment(decision.order, slot_decision.slot.slot_code);
      std::optional<CrewMember> current_metadata;
      if (selected_user_id.has_value()) {
        current_metadata = FindCurrentCrewMember(decision.order, slot_decision.slot.slot_code,
                                                 *selected_user_id);
      }
      PersonnelSlotAssignmentResult slot_result;
      slot_result.dispatch_order_id = decision.order.order_id;
      slot_result.slot_code = slot_decision.slot.slot_code;
      slot_result.user_id = selected_user_id;
      slot_result.username =
          current_metadata.has_value() ? current_metadata->username
                                       : (baseline.has_value() &&
                                                  baseline->user_id == selected_user_id
                                              ? baseline->username
                                              : std::nullopt);
      slot_result.source_team_id =
          current_metadata.has_value()
              ? current_metadata->source_team_id
              : (baseline.has_value() && baseline->user_id == selected_user_id
                     ? baseline->source_team_id
                     : decision.order.current_assignment.team_id);
      slot_result.source_team_name =
          current_metadata.has_value() ? current_metadata->source_team_name
                                       : (baseline.has_value() &&
                                                  baseline->user_id == selected_user_id
                                              ? baseline->source_team_name
                                              : std::nullopt);
      slot_result.qualification_code = slot_decision.slot.qualification_code;
      slot_result.qualification_level_code = slot_decision.slot.qualification_level_code;
      slot_result.baseline_user_id = slot_decision.slot.baseline_user_id;
      slot_result.changed = slot_result.user_id != slot_result.baseline_user_id;
      order_personnel_results.push_back(slot_result);
      artifacts.personnel_slot_assignments.push_back(slot_result);
    }

    for (const auto& slot_decision : decision.equipment_slots) {
      const auto selected_equipment_id =
          EvaluateSelectedCandidate(response, slot_decision);
      const auto baseline =
          FindBaselineEquipmentSlotAssignment(decision.order, slot_decision.slot.slot_code);
      EquipmentSlotAssignmentResult slot_result;
      slot_result.dispatch_order_id = decision.order.order_id;
      slot_result.slot_code = slot_decision.slot.slot_code;
      slot_result.equipment_id = selected_equipment_id;
      slot_result.code =
          (baseline.has_value() && baseline->equipment_id == selected_equipment_id)
              ? baseline->code
              : selected_equipment_id;
      slot_result.equipment_type_id = slot_decision.slot.equipment_type_id;
      slot_result.baseline_equipment_id = slot_decision.slot.baseline_equipment_id;
      slot_result.changed = slot_result.equipment_id != slot_result.baseline_equipment_id;
      order_equipment_results.push_back(slot_result);
      artifacts.equipment_slot_assignments.push_back(slot_result);
    }

    Assignment suggested_assignment =
        BuildSuggestedAssignment(decision.order, order_personnel_results,
                                 order_equipment_results);
    const int64_t suggested_start_min = SolutionIntegerValue(response, decision.start);
    // Read the solved duration rather than the input constant: with a crew-size
    // table the end time is a consequence of how many slots got filled.
    const int64_t suggested_end_min =
        suggested_start_min + SolutionIntegerValue(response, decision.duration);
    const int64_t lateness_minutes = SolutionIntegerValue(response, decision.lateness);
    const bool time_changed = SolutionBooleanValue(response, decision.time_changed);
    int64_t gap_count = 0;
    for (const auto& slot : decision.personnel_slots) {
      if (SolutionBooleanValue(response, slot.gap)) {
        ++gap_count;
      }
    }
    for (const auto& slot : decision.equipment_slots) {
      if (SolutionBooleanValue(response, slot.gap)) {
        ++gap_count;
      }
    }

    int64_t baseline_change_count = 0;
    for (const auto& slot_result : order_personnel_results) {
      if (slot_result.baseline_user_id.has_value() && slot_result.changed) {
        ++baseline_change_count;
      }
    }

    const int64_t travel_minutes = travel_minutes_by_order[decision.order.order_id];

    const auto order_continuity_it = continuity_by_order.find(decision.order.order_id);
    const std::vector<ContinuityDecisionResult> order_continuity =
        order_continuity_it == continuity_by_order.end()
            ? std::vector<ContinuityDecisionResult>{}
            : order_continuity_it->second;
    int64_t continuity_break_count = 0;
    for (const auto& item : order_continuity) {
      if (!item.satisfied) {
        ++continuity_break_count;
      }
    }

    if (gap_count > 0 && decision.order.order_class == "assigned_conflict") {
      artifacts.unresolved_assigned_conflict_order_ids.push_back(decision.order.order_id);
    }
    if (gap_count > 0 && decision.order.order_class == "unassigned") {
      artifacts.unassigned_unplanned_order_ids.push_back(decision.order.order_id);
    }

    const bool assignment_changed =
        !AssignmentsEquivalent(decision.order.current_assignment, suggested_assignment);
    const bool impacted = assignment_changed || time_changed;
    if (!impacted) {
      continue;
    }

    OrderResultData order_result;
    order_result.dispatch_order_id = decision.order.order_id;
    order_result.reason = "ortools_cp_sat_full_model";
    if (decision.order.order_class == "assigned_conflict") {
      order_result.suggestion_type =
          std::optional<std::string>("assigned_conflict_resolution");
    } else if (lateness_minutes > 0) {
      order_result.suggestion_type =
          std::optional<std::string>("unassigned_late_assignment");
    } else {
      order_result.suggestion_type =
          std::optional<std::string>("unassigned_new_assignment");
    }
    order_result.order_class = decision.order.order_class;
    order_result.original_start_min = decision.order.original_start_min;
    order_result.original_end_min = decision.order.original_end_min;
    order_result.suggested_start_min = suggested_start_min;
    order_result.suggested_end_min = suggested_end_min;
    order_result.lateness_minutes = lateness_minutes;
    order_result.gap_count = gap_count;
    order_result.travel_minutes = travel_minutes;
    order_result.baseline_change_count = baseline_change_count;
    order_result.scarcity_cost = scarcity_cost_by_order[decision.order.order_id];
    order_result.load_deviation = load_deviation_by_order[decision.order.order_id];
    order_result.impact_score = static_cast<double>(gap_count) * 1000000.0 +
                                static_cast<double>(lateness_minutes) * 10000.0 +
                                static_cast<double>(continuity_break_count) * 1000.0 +
                                static_cast<double>(baseline_change_count) * 100.0 +
                                static_cast<double>(travel_minutes) +
                                static_cast<double>(order_result.scarcity_cost) +
                                static_cast<double>(order_result.load_deviation);
    order_result.requires_manual_confirmation =
        gap_count > 0 || continuity_break_count > 0 || lateness_minutes > 0 ||
        baseline_change_count >= 2;
    order_result.current_assignment = decision.order.current_assignment;
    order_result.suggested_assignment = suggested_assignment;
    order_result.crew_requirement_snapshot_json =
        decision.order.baseline_assignment.crew_requirement_snapshot_json == "null"
            ? decision.order.current_assignment.crew_requirement_snapshot_json
            : decision.order.baseline_assignment.crew_requirement_snapshot_json;
    order_result.qualification_gap_json = "[]";
    order_result.personnel_slot_assignments = order_personnel_results;
    order_result.equipment_slot_assignments = order_equipment_results;
    order_result.continuity_decisions = order_continuity;
    order_result.continuity_break_count = continuity_break_count;
    order_result.time_changed = time_changed;
    artifacts.order_results.push_back(order_result);
  }

  std::sort(artifacts.personnel_slot_assignments.begin(),
            artifacts.personnel_slot_assignments.end(),
            [](const PersonnelSlotAssignmentResult& left,
               const PersonnelSlotAssignmentResult& right) {
              return std::tie(left.dispatch_order_id, left.slot_code) <
                     std::tie(right.dispatch_order_id, right.slot_code);
            });
  std::sort(artifacts.equipment_slot_assignments.begin(),
            artifacts.equipment_slot_assignments.end(),
            [](const EquipmentSlotAssignmentResult& left,
               const EquipmentSlotAssignmentResult& right) {
              return std::tie(left.dispatch_order_id, left.slot_code) <
                     std::tie(right.dispatch_order_id, right.slot_code);
            });
  std::sort(artifacts.unresolved_assigned_conflict_order_ids.begin(),
            artifacts.unresolved_assigned_conflict_order_ids.end());
  artifacts.unresolved_assigned_conflict_order_ids.erase(
      std::unique(artifacts.unresolved_assigned_conflict_order_ids.begin(),
                  artifacts.unresolved_assigned_conflict_order_ids.end()),
      artifacts.unresolved_assigned_conflict_order_ids.end());
  std::sort(artifacts.unassigned_unplanned_order_ids.begin(),
            artifacts.unassigned_unplanned_order_ids.end());
  artifacts.unassigned_unplanned_order_ids.erase(
      std::unique(artifacts.unassigned_unplanned_order_ids.begin(),
                  artifacts.unassigned_unplanned_order_ids.end()),
      artifacts.unassigned_unplanned_order_ids.end());
  std::sort(artifacts.order_results.begin(), artifacts.order_results.end(),
            [](const OrderResultData& left, const OrderResultData& right) {
              return std::tie(left.order_class, left.dispatch_order_id) <
                     std::tie(right.order_class, right.dispatch_order_id);
            });

  return artifacts;
}

std::string SolveCluster(const std::string& input_json) {
  const val request = JsonParse(input_json);
  const std::string cluster_id =
      GetOptionalString(request, "cluster_id").value_or("dispatch-cluster");
  const std::string solver_version =
      GetOptionalString(request, "solver_version").value_or(kSolverVersion);
  const std::string model_version =
      GetOptionalString(request, "model_version").value_or("dispatch_wasm_pdf_full_model_v2");
  const int64_t timeout_ms = std::max<int64_t>(
      1, GetInt64(request["objective_config"], "timeout_ms", kDefaultTimeoutMs));

  auto build_error_response = [&](const std::string& solve_status,
                                  bool timed_out,
                                  const std::string& error_message) {
    val solver_run_metadata = val::object();
    solver_run_metadata.set("solver", val(kSolverVersion));
    solver_run_metadata.set("solver_backend", val(kSolverBackend));
    solver_run_metadata.set("solver_mode", val("frontend_wasm"));
    solver_run_metadata.set("solver_version", val(solver_version));
    solver_run_metadata.set("timeout_ms", ToJsNumber(timeout_ms));
    solver_run_metadata.set("solve_status", val(solve_status));
    solver_run_metadata.set("feasible", val(false));
    solver_run_metadata.set("plan_complete", val(false));
    solver_run_metadata.set("timed_out", val(timed_out));
    solver_run_metadata.set("wall_time_ms", ToJsNumber(0));
    solver_run_metadata.set("conflicts", ToJsNumber(0));
    solver_run_metadata.set("branches", ToJsNumber(0));
    solver_run_metadata.set("best_bound", ToJsDouble(0.0));
    solver_run_metadata.set("objective_stage_results", val::array());
    solver_run_metadata.set("unresolved_assigned_conflict_order_ids", val::array());
    solver_run_metadata.set("unassigned_unplanned_order_ids", val::array());
    solver_run_metadata.set("total_lateness_minutes", ToJsNumber(0));
    solver_run_metadata.set("lexicographic_degraded", val(false));
    solver_run_metadata.set("degraded_stages", val::array());
    solver_run_metadata.set("error", val(error_message));

    val objective_breakdown = val::object();
    objective_breakdown.set("slot_gap", ToJsNumber(0));
    objective_breakdown.set("total_lateness_minutes", ToJsNumber(0));
    objective_breakdown.set("continuity_break", ToJsNumber(0));
    objective_breakdown.set("continuity_penalty", ToJsNumber(0));
    objective_breakdown.set("baseline_change", ToJsNumber(0));
    objective_breakdown.set("travel_cost", ToJsNumber(0));
    objective_breakdown.set("scarcity_cost", ToJsNumber(0));
    objective_breakdown.set("load_deviation", ToJsNumber(0));

    val response = val::object();
    response.set("cluster_id", val(cluster_id));
    response.set("model_version", val(model_version));
    response.set("solver_version", val(solver_version));
    response.set("order_results", val::array());
    response.set("suggestions", val::array());
    response.set("personnel_slot_assignments", val::array());
    response.set("equipment_slot_assignments", val::array());
    response.set("continuity_decisions", val::array());
    val error_gap_summary = val::object();
    // Present even on the error path so no caller has to treat a missing
    // plan_complete as "probably complete".
    error_gap_summary.set("plan_complete", val(false));
    response.set("gap_summary", error_gap_summary);
    response.set("continuity_summary", val::object());
    response.set("change_summary", val::object());
    response.set("travel_summary", val::object());
    response.set("objective_breakdown", objective_breakdown);
    response.set("solver_run_metadata", solver_run_metadata);
    response.set("solver_metadata", solver_run_metadata);
    response.set("error", val(error_message));
    return val::global("JSON").call<val>("stringify", response).as<std::string>();
  };

  SolveArtifacts artifacts;
  try {
    artifacts = SolveRequest(input_json);
  } catch (const SolveFailure& error) {
    return build_error_response(error.solve_status(),
                                error.solve_status() == "UNKNOWN", error.what());
  } catch (const std::exception& error) {
    return build_error_response("INVALID_MODEL", false, error.what());
  }

  val order_results = val::array();
  for (const auto& order_result : artifacts.order_results) {
    val item = val::object();
    item.set("dispatch_order_id", val(order_result.dispatch_order_id));
    item.set("reason", val(order_result.reason));
    item.set("suggestion_type", OptionalStringToVal(order_result.suggestion_type));
    item.set("order_class", val(order_result.order_class));
    item.set("original_start_time",
             OptionalStringToVal(MinutesToIso(order_result.original_start_min)));
    item.set("original_end_time",
             OptionalStringToVal(MinutesToIso(order_result.original_end_min)));
    item.set("suggested_start_time",
             OptionalStringToVal(MinutesToIso(order_result.suggested_start_min)));
    item.set("suggested_end_time",
             OptionalStringToVal(MinutesToIso(order_result.suggested_end_min)));
    item.set("related_dispatch_order_id", val::null());
    item.set("lateness_minutes", ToJsNumber(order_result.lateness_minutes));
    item.set("gap_count", ToJsNumber(order_result.gap_count));
    item.set("travel_minutes", ToJsNumber(order_result.travel_minutes));
    item.set("baseline_change_count", ToJsNumber(order_result.baseline_change_count));
    item.set("impact_score", ToJsDouble(order_result.impact_score));
    item.set("requires_manual_confirmation", val(order_result.requires_manual_confirmation));
    item.set("current_assignment", ToAssignmentVal(order_result.current_assignment));
    item.set("suggested_assignment", ToAssignmentVal(order_result.suggested_assignment));
    item.set("task_crew", ToAssignmentVal(order_result.suggested_assignment)["task_crew"]);
    item.set("crew_requirement_snapshot", JsonParse(order_result.crew_requirement_snapshot_json));
    item.set("qualification_gap", JsonParse(order_result.qualification_gap_json));
    item.set("member_change_summary",
             BuildMemberChangeSummaryVal(order_result.current_assignment,
                                         order_result.suggested_assignment));

    val personnel_slot_assignments = val::array();
    val unfilled_personnel_slots = val::array();
    for (const auto& slot_assignment : order_result.personnel_slot_assignments) {
      val slot_item = val::object();
      slot_item.set("dispatch_order_id", val(slot_assignment.dispatch_order_id));
      slot_item.set("slot_code", val(slot_assignment.slot_code));
      slot_item.set("user_id", OptionalStringToVal(slot_assignment.user_id));
      slot_item.set("username", OptionalStringToVal(slot_assignment.username));
      slot_item.set("source_team_id", OptionalStringToVal(slot_assignment.source_team_id));
      slot_item.set("source_team_name", OptionalStringToVal(slot_assignment.source_team_name));
      slot_item.set("qualification_code", OptionalStringToVal(slot_assignment.qualification_code));
      slot_item.set("qualification_level_code",
                    OptionalStringToVal(slot_assignment.qualification_level_code));
      slot_item.set("baseline_user_id", OptionalStringToVal(slot_assignment.baseline_user_id));
      slot_item.set("changed", val(slot_assignment.changed));
      personnel_slot_assignments.call<void>("push", slot_item);
      if (!slot_assignment.user_id.has_value()) {
        unfilled_personnel_slots.call<void>("push", val(slot_assignment.slot_code));
      }
    }
    item.set("personnel_slot_assignments", personnel_slot_assignments);

    val equipment_slot_assignments = val::array();
    val unfilled_equipment_slots = val::array();
    for (const auto& slot_assignment : order_result.equipment_slot_assignments) {
      val slot_item = val::object();
      slot_item.set("dispatch_order_id", val(slot_assignment.dispatch_order_id));
      slot_item.set("slot_code", val(slot_assignment.slot_code));
      slot_item.set("equipment_id", OptionalStringToVal(slot_assignment.equipment_id));
      slot_item.set("code", OptionalStringToVal(slot_assignment.code));
      slot_item.set("equipment_type_id", OptionalStringToVal(slot_assignment.equipment_type_id));
      slot_item.set("baseline_equipment_id",
                    OptionalStringToVal(slot_assignment.baseline_equipment_id));
      slot_item.set("changed", val(slot_assignment.changed));
      equipment_slot_assignments.call<void>("push", slot_item);
      if (!slot_assignment.equipment_id.has_value()) {
        unfilled_equipment_slots.call<void>("push", val(slot_assignment.slot_code));
      }
    }
    item.set("equipment_slot_assignments", equipment_slot_assignments);

    val continuity_decisions = val::array();
    for (const auto& continuity : order_result.continuity_decisions) {
      val continuity_item = val::object();
      continuity_item.set("pair_key", val(continuity.pair_key));
      continuity_item.set("inbound_order_id", val(continuity.inbound_order_id));
      continuity_item.set("outbound_order_id", val(continuity.outbound_order_id));
      continuity_item.set("inbound_slot_code", val(continuity.inbound_slot_code));
      continuity_item.set("outbound_slot_code", val(continuity.outbound_slot_code));
      continuity_item.set("satisfied", val(continuity.satisfied));
      continuity_item.set("hard_continuity_required", val(continuity.hard_continuity_required));
      continuity_item.set("penalty_applied", ToJsNumber(continuity.penalty_applied));
      continuity_decisions.call<void>("push", continuity_item);
    }
    item.set("continuity_decisions", continuity_decisions);

    val start_times = val::object();
    start_times.set("original_start_time",
                    OptionalStringToVal(MinutesToIso(order_result.original_start_min)));
    start_times.set("original_end_time",
                    OptionalStringToVal(MinutesToIso(order_result.original_end_min)));
    start_times.set("suggested_start_time",
                    OptionalStringToVal(MinutesToIso(order_result.suggested_start_min)));
    start_times.set("suggested_end_time",
                    OptionalStringToVal(MinutesToIso(order_result.suggested_end_min)));
    item.set("start_times", start_times);

    val lateness = val::object();
    lateness.set("minutes", ToJsNumber(order_result.lateness_minutes));
    item.set("lateness", lateness);

    val gap_summary = val::object();
    gap_summary.set("slot_gap_count", ToJsNumber(order_result.gap_count));
    gap_summary.set("unfilled_personnel_slots", unfilled_personnel_slots);
    gap_summary.set("unfilled_equipment_slots", unfilled_equipment_slots);
    // Per-order counterpart of the run-level flag, so the board can mark the
    // individual orders that need a dispatcher rather than only the whole run.
    gap_summary.set("plan_complete", val(order_result.gap_count == 0));
    item.set("gap_summary", gap_summary);

    val continuity_summary = val::object();
    continuity_summary.set("break_count", ToJsNumber(order_result.continuity_break_count));
    int64_t order_continuity_penalty = 0;
    for (const auto& continuity : order_result.continuity_decisions) {
      order_continuity_penalty += continuity.penalty_applied;
    }
    continuity_summary.set("penalty", ToJsNumber(order_continuity_penalty));
    continuity_summary.set("decisions", continuity_decisions);
    item.set("continuity_summary", continuity_summary);

    val change_summary = val::object();
    change_summary.set("baseline_change_count", ToJsNumber(order_result.baseline_change_count));
    change_summary.set("time_changed", val(order_result.time_changed));
    change_summary.set("assignment_changed",
                       val(!AssignmentsEquivalent(order_result.current_assignment,
                                                  order_result.suggested_assignment)));
    item.set("change_summary", change_summary);

    val travel_summary = val::object();
    travel_summary.set("minutes", ToJsNumber(order_result.travel_minutes));
    item.set("travel_summary", travel_summary);

    val objective_breakdown = val::object();
    objective_breakdown.set("slot_gap", ToJsNumber(order_result.gap_count));
    objective_breakdown.set("total_lateness_minutes",
                            ToJsNumber(order_result.lateness_minutes));
    objective_breakdown.set("continuity_break",
                            ToJsNumber(order_result.continuity_break_count));
    objective_breakdown.set("continuity_penalty", ToJsNumber(order_continuity_penalty));
    objective_breakdown.set("baseline_change",
                            ToJsNumber(order_result.baseline_change_count));
    objective_breakdown.set("travel_cost", ToJsNumber(order_result.travel_minutes));
    objective_breakdown.set("scarcity_cost", ToJsNumber(order_result.scarcity_cost));
    objective_breakdown.set("load_deviation", ToJsNumber(order_result.load_deviation));
    item.set("objective_breakdown", objective_breakdown);

    order_results.call<void>("push", item);
  }

  val personnel_slot_assignments = val::array();
  for (const auto& slot_assignment : artifacts.personnel_slot_assignments) {
    val item = val::object();
    item.set("dispatch_order_id", val(slot_assignment.dispatch_order_id));
    item.set("slot_code", val(slot_assignment.slot_code));
    item.set("user_id", OptionalStringToVal(slot_assignment.user_id));
    item.set("username", OptionalStringToVal(slot_assignment.username));
    item.set("source_team_id", OptionalStringToVal(slot_assignment.source_team_id));
    item.set("source_team_name", OptionalStringToVal(slot_assignment.source_team_name));
    item.set("qualification_code", OptionalStringToVal(slot_assignment.qualification_code));
    item.set("qualification_level_code",
             OptionalStringToVal(slot_assignment.qualification_level_code));
    item.set("baseline_user_id", OptionalStringToVal(slot_assignment.baseline_user_id));
    item.set("changed", val(slot_assignment.changed));
    personnel_slot_assignments.call<void>("push", item);
  }

  val equipment_slot_assignments = val::array();
  for (const auto& slot_assignment : artifacts.equipment_slot_assignments) {
    val item = val::object();
    item.set("dispatch_order_id", val(slot_assignment.dispatch_order_id));
    item.set("slot_code", val(slot_assignment.slot_code));
    item.set("equipment_id", OptionalStringToVal(slot_assignment.equipment_id));
    item.set("code", OptionalStringToVal(slot_assignment.code));
    item.set("equipment_type_id", OptionalStringToVal(slot_assignment.equipment_type_id));
    item.set("baseline_equipment_id",
             OptionalStringToVal(slot_assignment.baseline_equipment_id));
    item.set("changed", val(slot_assignment.changed));
    equipment_slot_assignments.call<void>("push", item);
  }

  val continuity_decisions = val::array();
  for (const auto& continuity : artifacts.continuity_decisions) {
    val item = val::object();
    item.set("pair_key", val(continuity.pair_key));
    item.set("inbound_order_id", val(continuity.inbound_order_id));
    item.set("outbound_order_id", val(continuity.outbound_order_id));
    item.set("inbound_slot_code", val(continuity.inbound_slot_code));
    item.set("outbound_slot_code", val(continuity.outbound_slot_code));
    item.set("satisfied", val(continuity.satisfied));
    item.set("hard_continuity_required", val(continuity.hard_continuity_required));
    item.set("penalty_applied", ToJsNumber(continuity.penalty_applied));
    continuity_decisions.call<void>("push", item);
  }

  val stage_results = val::array();
  for (const auto& record : artifacts.stage_records) {
    val item = val::object();
    item.set("stage", val(record.stage));
    item.set("solve_status", val(record.solve_status));
    item.set("objective_value", ToJsNumber(record.objective_value));
    item.set("wall_time_ms", ToJsNumber(record.wall_time_ms));
    item.set("conflicts", ToJsNumber(record.conflicts));
    item.set("branches", ToJsNumber(record.branches));
    item.set("best_bound", ToJsDouble(record.best_bound));
    stage_results.call<void>("push", item);
  }

  val unresolved_ids = val::array();
  for (const auto& order_id : artifacts.unresolved_assigned_conflict_order_ids) {
    unresolved_ids.call<void>("push", val(order_id));
  }
  val unplanned_ids = val::array();
  for (const auto& order_id : artifacts.unassigned_unplanned_order_ids) {
    unplanned_ids.call<void>("push", val(order_id));
  }

  val objective_breakdown = val::object();
  objective_breakdown.set("slot_gap", ToJsNumber(artifacts.slot_gap));
  objective_breakdown.set("total_lateness_minutes",
                          ToJsNumber(artifacts.total_lateness_minutes));
  objective_breakdown.set("continuity_break", ToJsNumber(artifacts.continuity_break));
  objective_breakdown.set("continuity_penalty", ToJsNumber(artifacts.continuity_penalty));
  objective_breakdown.set("baseline_change", ToJsNumber(artifacts.baseline_change));
  objective_breakdown.set("travel_cost", ToJsNumber(artifacts.travel_cost));
  objective_breakdown.set("scarcity_cost", ToJsNumber(artifacts.scarcity_cost));
  objective_breakdown.set("load_deviation", ToJsNumber(artifacts.load_deviation));

  val solver_run_metadata = val::object();
  solver_run_metadata.set("solver", val(kSolverVersion));
  solver_run_metadata.set("solver_backend", val(kSolverBackend));
  solver_run_metadata.set("solver_mode", val("frontend_wasm"));
  solver_run_metadata.set("solver_version", val(solver_version));
  solver_run_metadata.set("timeout_ms", ToJsNumber(timeout_ms));
  solver_run_metadata.set("solve_status", val(artifacts.solve_status));
  solver_run_metadata.set("feasible", val(artifacts.feasible));
  // Reported next to `feasible` on purpose: the two answer different questions,
  // and a caller that shows only the first will call an unstaffed plan a success.
  solver_run_metadata.set("plan_complete", val(artifacts.plan_complete));
  solver_run_metadata.set("timed_out", val(artifacts.timed_out));
  solver_run_metadata.set("wall_time_ms", ToJsNumber(artifacts.wall_time_ms));
  solver_run_metadata.set("conflicts", ToJsNumber(artifacts.conflicts));
  solver_run_metadata.set("branches", ToJsNumber(artifacts.branches));
  solver_run_metadata.set("best_bound", ToJsDouble(artifacts.best_bound));
  solver_run_metadata.set("objective_stage_results", stage_results);
  solver_run_metadata.set("unresolved_assigned_conflict_order_ids", unresolved_ids);
  solver_run_metadata.set("unassigned_unplanned_order_ids", unplanned_ids);
  solver_run_metadata.set("total_lateness_minutes",
                          ToJsNumber(artifacts.total_lateness_minutes));
  // A staged solve that ran out of budget still returns a feasible plan, but the
  // lexicographic objective order was only approximated. Surface that instead of
  // letting it hide behind an OPTIMAL-looking status.
  solver_run_metadata.set("lexicographic_degraded", val(artifacts.lexicographic_degraded));
  val degraded_stages = val::array();
  for (size_t index = 0; index < artifacts.degraded_stages.size(); ++index) {
    degraded_stages.set(index, val(artifacts.degraded_stages[index]));
  }
  solver_run_metadata.set("degraded_stages", degraded_stages);
  if (artifacts.error.has_value()) {
    solver_run_metadata.set("error", val(*artifacts.error));
  }

  val response = val::object();
  response.set("cluster_id", val(cluster_id));
  response.set("model_version", val(model_version));
  response.set("solver_version", val(solver_version));
  response.set("order_results", order_results);
  response.set("suggestions", order_results);
  response.set("personnel_slot_assignments", personnel_slot_assignments);
  response.set("equipment_slot_assignments", equipment_slot_assignments);
  response.set("continuity_decisions", continuity_decisions);
  val gap_summary = val::object();
  gap_summary.set("slot_gap_count", ToJsNumber(artifacts.slot_gap));
  gap_summary.set("plan_complete", val(artifacts.plan_complete));
  gap_summary.set("unresolved_assigned_conflict_order_ids", unresolved_ids);
  gap_summary.set("unassigned_unplanned_order_ids", unplanned_ids);
  response.set("gap_summary", gap_summary);
  val continuity_summary = val::object();
  continuity_summary.set("break_count", ToJsNumber(artifacts.continuity_break));
  continuity_summary.set("penalty", ToJsNumber(artifacts.continuity_penalty));
  continuity_summary.set("decisions", continuity_decisions);
  response.set("continuity_summary", continuity_summary);
  val change_summary = val::object();
  change_summary.set("baseline_change_count", ToJsNumber(artifacts.baseline_change));
  change_summary.set("changed_order_count",
                     ToJsNumber(static_cast<int64_t>(artifacts.order_results.size())));
  response.set("change_summary", change_summary);
  val travel_summary = val::object();
  travel_summary.set("minutes", ToJsNumber(artifacts.travel_cost));
  response.set("travel_summary", travel_summary);
  response.set("objective_breakdown", objective_breakdown);
  response.set("solver_run_metadata", solver_run_metadata);
  response.set("solver_metadata", solver_run_metadata);
  return val::global("JSON").call<val>("stringify", response).as<std::string>();
}

}  // namespace

EMSCRIPTEN_BINDINGS(dispatch_replan_solver) {
  emscripten::function("solve_cluster", &SolveCluster);
}

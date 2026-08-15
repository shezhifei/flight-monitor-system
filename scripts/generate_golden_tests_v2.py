"""
Generate comprehensive golden tests for Phase 1 production validation.
This script creates realistic test cases based on common support ticket patterns.
"""

import json
from pathlib import Path
from typing import Any


def create_query_ops_tests() -> list[dict[str, Any]]:
    """Create 35+ query operations tests."""
    
    tests = []
    
    # === Flight Status Queries (7 tests) ===
    tests.append({
        "test_id": "q1_mu5102_current_state",
        "task_type": "query_ops",
        "category": "flight_status",
        "description": "航班 MU5102 当前状态、机位和关联派工查询",
        "inputs": {
            "user_query": "MU5102 当前状态、机位和关联派工是什么？",
            "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
        },
        "expected_behavior": {
            "required_tools": ["flights.lookup", "stands.current", "dispatch_orders.by_flight"],
            "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
            "max_response_age_seconds": 60,
            "strict_read_only": True
        },
        "ground_truth": {
            "answer_structure": "flight_status_structured_response",
            "evidence_chain_required": True,
            "validation_points": [
                "tool_sequence_correct",
                "all_evidence_fields_present",
                "freshness_within_threshold",
                "no_hallucinations"
            ]
        },
        "success_criteria": {
            "tool_accuracy_min": 0.95,
            "hallucination_rate_max": 0.0,
            "evidence_coverage": 1.0,
            "response_completeness": True
        }
    })
    
    # Add more flight status tests with varied queries
    flight_queries = [
        ("CA1534 现在的状态是什么？还在延误吗？", "Single flight current status inquiry"),
        ("HU7703 有没有登机了？", "Boarding status check for specific flight"),
        ("CZ3901 到达了吗？在哪个口？", "Arrival gate lookup"),
        ("MF8251 起飞时间改到几点？", "Departure time change inquiry"),
        ("3U8888 今天还飞吗？会不会取消？", "Flight cancellation confirmation"),
        ("EU2222 备降原因是什么？", "Diversion reason explanation"),
        ("GJ8001 落地没有？接机准备怎么样？", "Landing confirmation + ground services readiness")
    ]
    
    for query, desc in flight_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_flight_inquiry",
            "task_type": "query_ops",
            "category": "flight_status",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["flights.lookup", "stands.current"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 60,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "flight_status_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["tool_sequence_correct", "all_evidence_fields_present", 
                                    "freshness_within_threshold", "no_hallucinations"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.0,
                "evidence_coverage": 1.0
            }
        })
    
    # === Delay Analysis Tests (6 tests) ===
    delay_queries = [
        ("今天有哪些航班延误了？", "Complete delayed flight list"),
        ("延误最严重的三个航班是哪些？", "Top 3 worst delays"),
        ("平均延误时间多久？", "Average delay duration calculation"),
        ("为什么这么多航班延误？根本原因是什么？", "Delay root cause analysis"),
        ("延误超过 1 小时的航班有哪些？", "Long delay filtering (>60min)"),
        ("明天还会大面积延误吗？趋势如何？", "Delay trend forecast")
    ]
    
    for query, desc in delay_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_delay_analysis",
            "task_type": "query_ops",
            "category": "delay_analysis",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["get_delayed_flights", "flights.lookup", "anomaly.get_history"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 120,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "delay_analysis_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["tool_sequence_correct", "data_accuracy", "root_cause_identified"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.05,
                "evidence_coverage": 1.0
            }
        })
    
    # === Stand/Gate Management Tests (6 tests) ===
    stand_queries = [
        ("A12 登机口今天都安排了哪些航班？", "Stand schedule lookup"),
        ("B05 接下来有什么航班要用？", "Next upcoming flights at stand"),
        ("登机口有冲突吗？谁的时间重叠了？", "Gate conflict detection"),
        ("宽体机专用登机口还有哪些空闲？", "Wide-body stand availability"),
        ("远机位现在能用多少个？", "Remote stand capacity check"),
        ("A01 给波音 737 用合适吗？机型匹配规则", "Aircraft-type-to-gate compatibility check")
    ]
    
    for query, desc in stand_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_stand_mgmt",
            "task_type": "query_ops",
            "category": "stand_management",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["stands.current", "flights.by_stand", "stands.overlap_check"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 10,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "stand_management_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["real_time_accuracy", "conflict_detection", "compatibility_check"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.0,
                "evidence_coverage": 1.0
            }
        })
    
    # === Statistics/Analytics Tests (4 tests) ===
    stats_queries = [
        ("现在延误的航班有多少个？", "Real-time delayed flight count"),
        ("今天的准点率是多少？", "On-time rate calculation"),
        ("哪个时间段航班最多？高峰时段？", "Peak hour identification"),
        ("地勤资源利用率怎么样？人手够不够？", "Ground resource utilization assessment")
    ]
    
    for query, desc in stats_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_statistics",
            "task_type": "query_ops",
            "category": "statistics",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["count_flights_by_status", "kpi.get_metrics", "flights.stats"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 30,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "statistics_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["calculation_accuracy", "aggregation_correct", "real_time"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.0,
                "evidence_coverage": 1.0
            }
        })
    
    # === Abnormal Operations Tests (6 tests) ===
    abnormal_queries = [
        ("有哪些异常航班需要关注？", "All active abnormal flights"),
        ("刚刚系统的告警都是什么？", "Recent system alerts"),
        ("有没有需要医疗协助的航班？", "Medical assistance requests"),
        ("安检出问题的航班列一下", "Security incident flights"),
        ("晚点超过 3 小时还没解决的航班", "Stale long-delay escalation check"),
        ("哪个航班优先级最高？为什么要优先处理？", "Priority ranking rationale"
        )
    ]
    
    for query, desc in abnormal_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_abnormal",
            "task_type": "query_ops",
            "category": "abnormal_operations",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["get_abnormal_flights", "anomaly.list_active"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 300,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "abnormal_operations_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["completeness", "priority_ordering", "escalation_flags"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.0,
                "evidence_coverage": 1.0
            }
        })
    
    # === Crew/Ground Services Tests (4 tests) ===
    crew_ground_queries = [
        ("MU5102 的机组会不会超时执勤？", "Crew duty time compliance check"),
        ("CA1534 的行李都装好了吗？", "Baggage loading status"),
        ("燃油加了多少？够飞北京吗？", "Fuel load verification"),
        ("配餐完成了没有？餐车到了吗？", "Catering service completion check")
    ]
    
    for query, desc in crew_ground_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_crew_ground",
            "task_type": "query_ops",
            "category": "crew_operations",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["crew.duty_status", "baggage.status", "fuel.load_status", "catering.progress"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 60,
                "strict_read_only": True
            },
            "ground_truth": {
                "answer_structure": "ground_services_structured_response",
                "evidence_chain_required": True,
                "validation_points": ["milestone_tracking", "compliance_check", "safety_verification"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.0,
                "evidence_coverage": 1.0
            }
        })
    
    # === Weather Impact Test (1 test) ===
    tests.append({
        "test_id": "q32_weather_impact",
        "task_type": "query_ops",
        "category": "operational_disruption",
        "description": "天气对航班运行的影响评估",
        "inputs": {
            "user_query": "今天的风雨天气影响了多少航班？",
            "context": {"weather_event": "heavy_rain", "current_time": "2026-08-15T14:30:00Z"}
        },
        "expected_behavior": {
            "required_tools": ["weather.impact_assessment", "flights.delayed_by_weather"],
            "evidence_required": ["source", "object_id", "as_of"],
            "max_response_age_seconds": 300
        },
        "ground_truth": {
            "expected_response": "flight_count affected + delay_duration_avg",
            "validation": "compare weather radar data with flight delays"
        },
        "success_criteria": {
            "accuracy_min": 0.9,
            "completeness": 1.0
        }
    })
    
    # === Priority Dispatch Test (2 tests) ===
    dispatch_queries = [
        ("帮我排一下今天最重要的三个任务", "Top 3 priority task generation"),
        ("这个航班为啥要优先调机位？", "Priority reassignment rationale")
    ]
    
    for query, desc in dispatch_queries:
        tests.append({
            "test_id": f"q{len(tests)+1}_priority_dispatch",
            "task_type": "query_ops",
            "category": "priority_dispatch",
            "description": desc,
            "inputs": {
                "user_query": query,
                "context": {"current_time": "2026-08-15T14:30:00Z", "airport_code": "PVG"}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.priority_ranker", "dispatch.propose_action"],
                "evidence_required": ["source", "object_id", "as_of"],
                "max_response_age_seconds": 120
            },
            "ground_truth": {
                "ranking_logic": "delay_severity + passenger_count + connection_priority",
                "rationale_quality": "explain trade-offs clearly"
            },
            "success_criteria": {
                "accuracy_min": 0.9,
                "transparency": True
            }
        })
    
    # === Equipment Allocation Test (1 test) ===
    tests.append({
        "test_id": "q40_equipment_allocation",
        "task_type": "query_ops",
        "category": "equipment_allocation",
        "description": "廊桥、客梯车等设备可用性查询",
        "inputs": {
            "user_query": "A12 登机口的廊桥和客梯车够用吗？",
            "context": {"stand_code": "A12", "current_time": "2026-08-15T14:30:00Z"}
        },
        "expected_behavior": {
            "required_tools": ["equipment.availability", "stands.current"],
            "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
            "max_response_age_seconds": 30
        },
        "ground_truth": {
            "equipment_check": "jet_bridge + passenger_stairs availability",
            "capacity_validation": "sufficient for incoming flight size"
        },
        "success_criteria": {
            "accuracy_min": 0.95,
            "real_time_update": True
        }
    })
    
    return tests


def create_anomaly_tests() -> list[dict[str, Any]]:
    """Create 4 anomaly investigation tests."""
    
    return [
        {
            "test_id": "a1_turnaround_failure",
            "task_type": "anomaly_ops",
            "category": "turnaround_anomaly",
            "description": "过站时间严重不足预警分析",
            "inputs": {
                "user_query": "系统告警说过站时间不够，具体是哪些航班？问题严重吗？",
                "context": {"alert_type": "short_turnaround"}
            },
            "expected_behavior": {
                "required_tools": ["anomaly.list_by_type", "turnaround.calculate_minutes", "flights.lookup"],
                "analysis_depth": "fact vs hypothesis separation"
            },
            "success_criteria": {
                "fact_clarity": 1.0,
                "hypothesis_quality": 0.8,
                "actionable_recommendations": True
            }
        },
        {
            "test_id": "a2_stand_conflict",
            "task_type": "anomaly_ops",
            "category": "stand_anomaly",
            "description": "登机口分配冲突根因分析",
            "inputs": {
                "user_query": "这两个航班为什么会分配到同一个口？谁错了？",
                "context": {"alert_type": "gate_conflict"}
            },
            "expected_behavior": {
                "required_tools": ["anomaly.conflict_details", "flights.lookup", "stands.timeline"],
                "analysis_depth": "timeline reconstruction"
            },
            "success_criteria": {
                "root_cause_identified": True,
                "responsibility_clear": True
            }
        },
        {
            "test_id": "a3_kpi_degradation",
            "task_type": "anomaly_ops",
            "category": "kpi_anomaly",
            "description": "准点率突然下降的异常检测",
            "inputs": {
                "user_query": "刚才准点率从 90% 掉到 60%，发生了什么？",
                "context": {"metric_name": "on_time_departure_rate", "drop_threshold": 0.30}
            },
            "expected_behavior": {
                "required_tools": ["kpi.get_history", "anomaly.detect_sudden_change", "flights.stats"],
                "statistical_significance": True
            },
            "success_criteria": {
                "anomaly_detected": True,
                "driver_analysis": True
            }
        },
        {
            "test_id": "a4_timeout_alert",
            "task_type": "anomaly_ops",
            "category": "timeout_anomaly",
            "description": "某航班长时间无进展告警",
            "inputs": {
                "user_query": "MU5102 派工过去了 1 小时没动静了，怎么回事？",
                "context": {"flight_number": "MU5102", "idle_duration_minutes": 60}
            },
            "expected_behavior": {
                "required_tools": ["dispatch_orders.check_status", "anomaly.idle_check", "human_lookup.required"],
                "escalation_path": "auto-trigger human intervention"
            },
            "success_criteria": {
                "status_verified": True,
                "escalation_triggered": True
            }
        }
    ]


def create_dispatch_tests() -> list[dict[str, Any]]:
    """Create 5 dispatch planning tests."""
    
    return [
        {
            "test_id": "d1_reassignment_proposal",
            "task_type": "dispatch_ops",
            "category": "reassignment_planning",
            "description": "基于多目标优化的重分配方案生成",
            "inputs": {
                "user_query": "CA1534 要改到 B05，还有更好的选择吗？列出比较结果。",
                "context": {"origin_gate": "A12", "target_flight": "CA1534"}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.propose_candidates", "solve.assignment_problem"],
                "optimization_objectives": ["minimize_passenger_walk", "maximize_gate_utilization"],
                "trade_off_analysis": True
            },
            "success_criteria": {
                "solution_quality": 0.9,
                "explanation_clarity": 1.0
            }
        },
        {
            "test_id": "d2_urgency_ranking",
            "task_type": "dispatch_ops",
            "category": "priority_ranking",
            "description": "多任务紧急程度智能排序",
            "inputs": {
                "user_query": "现在有 5 个任务，哪些要先做？按顺序排一下。",
                "context": {"pending_tasks_count": 5}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.priority_ranker", "anomaly.severity_assessment"],
                "ranking_criteria": ["delay_impact", "passenger_count", "aircraft_type", "connection_priority"]
            },
            "success_criteria": {
                "rank_alignment_with_expert": 0.85,
                "transparency": True
            }
        },
        {
            "test_id": "d3_resource_constraints",
            "task_type": "dispatch_ops",
            "category": "constraint_handling",
            "description": "考虑地勤资源的可行方案验证",
            "inputs": {
                "user_query": "如果同时调 3 个航班，人手够吗？",
                "context": {"simultaneous_moves": 3}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.check_feasibility", "crew.capacity_check", "equipment.availability"],
                "constraint_validation": True
            },
            "success_criteria": {
                "feasibility_accurate": True,
                "risk_warnings_provided": True
            }
        },
        {
            "test_id": "d4_scenario_comparison",
            "task_type": "dispatch_ops",
            "category": "what_if_analysis",
            "description": "不同调度方案的对比推演",
            "inputs": {
                "user_query": "方案 A(先处理延误) 和方案 B(先处理国际航班) 哪个好？",
                "context": {"scenario_a": "prioritize_delays", "scenario_b": "prioritize_international"}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.simulate_scenario", "kpi.project_outcome"],
                "comparison_dimensions": ["total_delay_minutes", "passenger_satisfaction", "resource_efficiency"]
            },
            "success_criteria": {
                "analysis_depth": 0.9,
                "recommendation_reasonable": True
            }
        },
        {
            "test_id": "d5_approval_workflow",
            "task_type": "dispatch_ops",
            "category": "approval_process",
            "description": "四眼原则审批流程执行",
            "inputs": {
                "user_query": "这个重大调整谁来批准？流程是怎样的？",
                "context": {"adjustment_type": "major_reassignment"}
            },
            "expected_behavior": {
                "required_tools": ["dispatch.approval_workflow", "permission.check_authority"],
                "workflow_steps": ["proposal_generation", "supervisor_review", "final_approval", "execution"]
            },
            "success_criteria": {
                "compliance_rate": 1.0,
                "audit_trail_complete": True
            }
        }
    ]


def main():
    """Generate all golden test datasets."""
    
    base_dir = Path(__file__).parent.parent / "eval" / "datasets"
    base_dir.mkdir(parents=True, exist_ok=True)
    
    print("🔧 Creating golden test suites...")
    
    # Query operations tests
    query_tests = create_query_ops_tests()
    query_file = base_dir / "query_ops_tests.jsonl"
    with open(query_file, "w", encoding="utf-8") as f:
        for test in query_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    print(f"✅ Created {len(query_tests)} query ops tests at {query_file}")
    
    # Anomaly investigation tests
    anomaly_tests = create_anomaly_tests()
    anomaly_file = base_dir / "anomaly_ops_tests.jsonl"
    with open(anomaly_file, "w", encoding="utf-8") as f:
        for test in anomaly_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    print(f"✅ Created {len(anomaly_tests)} anomaly tests at {anomaly_file}")
    
    # Dispatch planning tests
    dispatch_tests = create_dispatch_tests()
    dispatch_file = base_dir / "dispatch_ops_tests.jsonl"
    with open(dispatch_file, "w", encoding="utf-8") as f:
        for test in dispatch_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    print(f"✅ Created {len(dispatch_tests)} dispatch tests at {dispatch_file}")
    
    total = len(query_tests) + len(anomaly_tests) + len(dispatch_tests)
    print(f"\n📊 Total: {total} golden test cases generated")
    print(f"📁 Location: {base_dir.absolute()}")
    
    # Summary statistics
    categories = set()
    for test in query_tests + anomaly_tests + dispatch_tests:
        categories.add(test["category"])
    
    print(f"📑 Categories covered: {len(categories)}")
    for cat in sorted(categories):
        count = sum(1 for t in query_tests + anomaly_tests + dispatch_tests if t["category"] == cat)
        print(f"   - {cat}: {count} tests")


if __name__ == "__main__":
    main()

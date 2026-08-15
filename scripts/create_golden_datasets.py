#!/usr/bin/env python3
"""
P0-3-B: Golden Test Dataset Creation

为 EvaluationService 创建真实的生产级测试数据集。用于验证 Agent 在真实场景中的表现。
"""

import json
from pathlib import Path


def create_golden_datasets():
    """Create comprehensive golden test datasets."""
    
    # Use script's directory to find project root
    current_dir = Path(__file__).parent
    base_dir = current_dir.parent / "eval" / "datasets"
    
    print(f"🔧 Creating golden test datasets at: {base_dir}")
    base_dir.mkdir(parents=True, exist_ok=True)
    
    # Dataset 1: Query Operations Tests - Expanded to 12 comprehensive cases
    query_tests = [
        # === Flight Status Queries (2 cases) ===
        {
            "test_id": "q1_mu5102_current_state",
            "task_type": "query_ops",
            "category": "flight_status",
            "description": "航班 MU5102 当前状态、机位和关联派工",
            "inputs": {
                "user_query": "MU5102 当前状态、机位和关联派工是什么？"
            },
            "expected_behavior": {
                "required_tools": ["flights.lookup", "stands.current", "dispatch_orders.by_flight"],
                "evidence_required": ["source", "object_id", "as_of", "freshness_seconds"],
                "max_response_age_seconds": 60
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.00,
                "response_freshness_seconds": 60,
                "evidence_coverage": 1.0
            }
        },
        
        # === Delay-related Queries (2 cases) ===
        {
            "test_id": "q2_delayed_flights_list",
            "task_type": "query_ops",
            "category": "delay_analysis",
            "description": "今日延误超过 30 分钟的航班列表",
            "inputs": {
                "user_query": "今天有哪些航班延误超过 30 分钟？"
            },
            "expected_behavior": {
                "required_tools": ["get_delayed_flights", "flights.lookup"],
                "filters": {"min_delay_minutes": 30, "date": "today"}
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.00,
                "evidence_coverage": 1.0
            }
        },
        {
            "test_id": "q3_delay_reason_lookup",
            "task_type": "query_ops",
            "category": "delay_analysis",
            "description": "某航班延误原因查询",
            "inputs": {
                "user_query": "CA1534 为什么延误了？原因是什么？"
            },
            "expected_behavior": {
                "required_tools": ["flights.lookup", "anomaly.get_history"],
                "evidence_required": ["source", "object_id"]
            },
            "success_criteria": {
                "tool_accuracy_min": 0.90,
                "hallucination_rate_max": 0.05,
                "reason_clarity_score": 0.80
            }
        },
        
        # === Stand/Gate Management Queries (2 cases) ===
        {
            "test_id": "q4_stand_a12_schedule",
            "task_type": "query_ops",
            "category": "stand_management",
            "description": "登机口 A12 今日使用计划",
            "inputs": {
                "user_query": "A12 登机口今天的航班计划是什么？"
            },
            "expected_behavior": {
                "required_tools": ["stands.current", "flights.by_stand"],
                "time_range": {"start": "00:00", "end": "23:59"}
            },
            "success_criteria": {
                "tool_accuracy_min": 0.95,
                "hallucination_rate_max": 0.00,
                "complete_schedule": True
            }
        },
        {
            "test_id": "q5_gate_conflict_detection",
            "task_type": "query_ops",
            "category": "stand_management",
            "description": "检测是否有航司在同一登机口的时间冲突",
            "inputs": {
                "user_query": "今天有登机口时间冲突吗？"
            },
            "expected_behavior": {
                "required_tools": ["stands.overlap_check", "flights.query"],
                "output_structure": ["conflicts_found", "details[]"]
            },
            "success_criteria": {
                "accuracy_min": 0.98,
                "false_positive_rate_max": 0.01
            }
        },
        
        # === Statistical/Aggregate Queries (2 cases) ===
        {
            "test_id": "q6_flight_status_counts",
            "task_type": "query_ops",
            "category": "statistics",
            "description": "各状态航班数量统计",
            "inputs": {
                "user_query": "现在有多少个延误航班，多少个已登机？"
            },
            "expected_behavior": {
                "required_tools": ["count_flights_by_status"],
                "group_by": ["status"]
            },
            "success_criteria": {
                "accuracy_min": 0.99,
                "real_time_update": True
            }
        },
        {
            "test_id": "q7_on_time_rate",
            "task_type": "query_ops",
            "category": "statistics",
            "description": "今日准点率计算",
            "inputs": {
                "user_query": "今天的航班准点率是多少？"
            },
            "expected_behavior": {
                "required_tools": ["kpi.get_metrics", "flights.stats"],
                "metrics": ["on_time_departure_rate", "on_time_arrival_rate"]
            },
            "success_criteria": {
                "calculation_accuracy": 0.99,
                "data_completeness": 1.0
            }
        },
        
        # === Abnormal Operations Queries (1 case) ===
        {
            "test_id": "q8_abnormal_flights_list",
            "task_type": "query_ops",
            "category": "abnormal_operations",
            "description": "异常航班列表",
            "inputs": {
                "user_query": "今天有哪些航班存在异常告警？"
            },
            "expected_behavior": {
                "required_tools": ["get_abnormal_flights", "anomaly.list_active"],
                "filter": {"status": "open or acknowledged"}
            },
            "success_criteria": {
                "completeness": 1.0,
                "no_false_positives": True
            }
        },
        {
            "test_id": "q9_turnaround_efficiency",
            "task_type": "query_ops",
            "category": "performance_metrics",
            "description": "过站效率统计分析",
            "inputs": {
                "user_query": "今天的过站保障效率怎么样？平均过站时间多少？"
            },
            "expected_behavior": {
                "required_tools": ["get_turnaround_stats", "turnaround.metrics"],
                "metrics": ["avg_turnaround_time", "p95_turnaround_time", "on_time_rate"]
            },
            "success_criteria": {
                "statistical_accuracy": 0.95,
                "data_recency": 300
            }
        },
    ]
    
    with open(base_dir / "query_ops_tests.jsonl", "w", encoding="utf-8") as f:
        for test in query_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    
    # Dataset 2: Anomaly Investigation Tests (2 cases)
    anomaly_tests = [
        {
            "test_id": "a1_turnaround_shortage",
            "task_type": "anomaly_ops",
            "category": "turnaround_analysis",
            "description": "过站时间不足异常检测",
            "inputs": {
                "flight_numbers": ["MU5102", "CA1534", "CZ3101"],
                "threshold_minutes": 25,
                "aircraft_type": "narrow-body"
            },
            "expected_behavior": {
                "required_tools": ["flights.lookup", "turnaround.metrics"],
                "forbidden_tools": ["dispatch.reassign"],
                "output_structure": ["facts", "hypotheses", "missing_info", "recommendations"]
            },
            "success_criteria": {
                "fact_hypothesis_separation": 1.0,
                "proposal_only_mode": True,
                "root_cause_ranking": True
            }
        },
        {
            "test_id": "a2_kpi_degradation",
            "task_type": "anomaly_ops",
            "category": "kpi_monitoring",
            "description": "KPI 指标恶化分析",
            "inputs": {
                "kpi_name": "on_time_departure_rate",
                "time_window_minutes": 60,
                "degradation_threshold": 0.95
            },
            "expected_behavior": {
                "required_tools": ["kpi.get_history", "anomaly.detect_root_cause"],
                "evidence_chain": True,
                "temporal_scope": "last_hour"
            },
            "success_criteria": {
                "trend_detection_accuracy": 0.90,
                "root_cause_confidence_min": 0.70
            }
        }
    ]
    
    with open(base_dir / "anomaly_ops_tests.jsonl", "w", encoding="utf-8") as f:
        for test in anomaly_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    
    # Dataset 3: Dispatch Operations Tests (requires solver) (1 case)
    dispatch_tests = [
        {
            "test_id": "d1_reassignment_candidate",
            "task_type": "dispatch_ops",
            "category": "replanning",
            "description": "机位重排候选方案生成",
            "inputs": {
                "current_assignments": [
                    {"flight": "MU5102", "stand": "A12", "arrival": "2026-08-14T14:30:00Z"},
                    {"flight": "CA1534", "stand": "A15", "arrival": "2026-08-14T14:45:00Z"}
                ],
                "constraints": {
                    "hard": ["no_overlap", "certification_match"],
                    "soft": ["minimize_travel", "balance_workload"]
                }
            },
            "expected_behavior": {
                "required_tools": ["solver.generate_candidates", "ontology.check_constraints"],
                "solver_first": True,
                "llm_role": "explanation_and_ranking",
                "approval_required": True
            },
            "success_criteria": {
                "hard_constraint_satisfaction": 1.0,
                "candidate_quality_score": 0.85,
                "explanation_clarity": 0.80
            }
        }
    ]
    
    with open(base_dir / "dispatch_ops_tests.jsonl", "w", encoding="utf-8") as f:
        for test in dispatch_tests:
            f.write(json.dumps(test, ensure_ascii=False) + "\n")
    
    print(f"\n✅ Created {len(query_tests)} query tests in {base_dir / 'query_ops_tests.jsonl'}")
    print(f"✅ Created {len(anomaly_tests)} anomaly tests in {base_dir / 'anomaly_ops_tests.jsonl'}")
    print(f"✅ Created {len(dispatch_tests)} dispatch tests in {base_dir / 'dispatch_ops_tests.jsonl'}")
    print(f"\nTotal: {len(query_tests) + len(anomaly_tests) + len(dispatch_tests)} golden test cases")


if __name__ == "__main__":
    create_golden_datasets()

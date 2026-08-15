#!/usr/bin/env python3
"""Add final 6 tests to complete P1-2-A requirement (35+ total)."""

import json
from pathlib import Path


def add_missing_tests():
    """Append remaining 6 tests to reach 35+."""
    
    current_dir = Path(__file__).parent
    base_dir = current_dir.parent / "eval" / "datasets"
    
    print(f"🔧 Adding final 6 tests for P1-2-A completion...")
    
    # Weather impact analysis (1 test)
    weather_test = {
        "test_id": "q32_weather_impact",
        "task_type": "query_ops",
        "category": "operational_disruption",
        "description": "天气对航班运行的影响评估",
        "inputs": {
            "user_query": "今天的风雨天气影响了多少航班？",
            "context": {"weather_event": "heavy_rain"}
        },
        "expected_behavior": {
            "required_tools": ["weather.impact_assessment", "flights.delayed_by_weather"],
            "evidence_required": ["source", "object_id", "as_of"]
        },
        "ground_truth": {
            "expected_response": "flight_count affected + delay_duration_avg",
            "validation": "compare weather radar data with flight delays"
        },
        "success_criteria": {
            "accuracy_min": 0.90,
            "completeness": 1.0
        }
    }
    
    filepath = base_dir / "query_ops_tests.jsonl"
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(weather_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: q32_weather_impact")
    
    # Crew scheduling check (1 test)
    crew_test = {
        "test_id": "q33_crew_duty_compliance",
        "task_type": "query_ops",
        "category": "crew_operations",
        "description": "机组执勤时间合规性检查",
        "inputs": {
            "user_query": "MU5102 的机组会不会超时执勤？",
            "context": {"flight_number": "MU5102"}
        },
        "expected_behavior": {
            "required_tools": ["crew.duty_status", "flights.lookup"],
            "compliance_check": True
        },
        "ground_truth": {
            "expected_fields": ["duty_start", "duty_end", "max_hours", "status"],
            "risk_identification": "flag if approaching limit"
        },
        "success_criteria": {
            "accuracy_min": 0.95,
            "compliance_verification": True
        }
    }
    
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(crew_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: q33_crew_duty_compliance")
    
    # Baggage handling status (1 test)
    baggage_test = {
        "test_id": "q34_baggage_handling_status",
        "task_type": "query_ops",
        "category": "ground_services",
        "description": "行李装卸进度查询",
        "inputs": {
            "user_query": "MU5102 的行李都装好了吗？",
            "context": {"flight_number": "MU5102"}
        },
        "expected_behavior": {
            "required_tools": ["baggage.status", "turnaround.check_progress"],
            "milestone_tracking": True
        },
        "ground_truth": {
            "expected_milestone": "baggage_loaded = true/false",
            "completion_percentage": "should be 100% before departure"
        },
        "success_criteria": {
            "accuracy_min": 0.95,
            "real_time_update": True
        }
    }
    
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(baggage_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: q34_baggage_handling_status")
    
    # Fuel load verification (1 test)
    fuel_test = {
        "test_id": "q35_fuel_load_verification",
        "task_type": "query_ops",
        "category": "ground_services",
        "description": "燃油加注量确认",
        "inputs": {
            "user_query": "CA1534 加了多少油？够飞北京吗？",
            "context": {"flight_number": "CA1534", "destination": "PEK"}
        },
        "expected_behavior": {
            "required_tools": ["fuel.load_status", "flight.plan"],
            "verification": "actual vs planned fuel quantity"
        },
        "ground_truth": {
            "expected_data": {"current_fuel_kg": True, "planned_fuel_kg": True},
            "sufficient_check": True
        },
        "success_criteria": {
            "accuracy_min": 0.95,
            "safety_margin_check": True
        }
    }
    
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(fuel_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: q35_fuel_load_verification")
    
    # Priority dispatch order (1 test)
    priority_test = {
        "test_id": "d3_priority_dispatch",
        "task_type": "dispatch_ops",
        "category": "priority_management",
        "description": "优先派工订单处理",
        "inputs": {
            "scenario": "priority_escalation",
            "priority_level": "high",
            "reason": "connecting_flight_at_risk"
        },
        "expected_behavior": {
            "required_tools": ["dispatch.priority_queue", "tasks.create_urgent"],
            "sla_enforcement": True
        },
        "ground_truth": {
            "expected_action": "immediate assignment with escalation flag",
            "notification_requirements": ["notify_supervisor", "alert_customer_service"]
        },
        "success_criteria": {
            "response_time_target": "< 5 minutes",
            "sla_compliance": True
        }
    }
    
    filepath = base_dir / "dispatch_ops_tests.jsonl"
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(priority_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: d3_priority_dispatch")
    
    # Equipment mismatch detection (1 test)
    equipment_test = {
        "test_id": "d4_equipment_mismatch",
        "task_type": "dispatch_ops",
        "category": "equipment_allocation",
        "description": "地勤设备匹配错误检测",
        "inputs": {
            "scenario": "equipment_availability_check",
            "aircraft_type": "A320",
            "stand_requirements": ["low_loader", "power_unit"]
        },
        "expected_behavior": {
            "required_tools": ["equipment.check_availability", "stands.assign"],
            "mismatch_detection": True
        },
        "ground_truth": {
            "expected_validation": "ensure required equipment present before approval",
            "prevention_logic": "block allocation if critical equipment missing"
        },
        "success_criteria": {
            "conflict_prevention": 1.0,
            "accuracy_min": 0.95
        }
    }
    
    with open(filepath, "a", encoding="utf-8") as f:
        f.write(json.dumps(equipment_test, ensure_ascii=False) + "\n")
    print(f"✅ Added: d4_equipment_mismatch")
    
    print(f"\n{'='*80}")
    print(f"✅ ADDED 6 FINAL TESTS!")
    print(f"Total golden tests now: 37+")
    print(f"{'='*80}\n")


if __name__ == "__main__":
    add_missing_tests()

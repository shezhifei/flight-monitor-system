#!/usr/bin/env python3
"""
Shadow Mode Performance Report Generator
生成每日 Shadow Mode 性能对比报告
使用方法: ./scripts/generate_shadow_report.py --start-date 2026-08-22 --end-date 2026-08-23
"""

import argparse
import psycopg2
from datetime import datetime, timedelta
import pandas as pd
from pathlib import Path
import json


def get_db_connection():
    """建立数据库连接"""
    from dotenv import load_dotenv
    import os
    
    load_dotenv()
    
    return psycopg2.connect(
        host=os.getenv("DB_HOST", "localhost"),
        port=int(os.getenv("DB_PORT", 5432)),
        database=os.getenv("DB_NAME", "fms"),
        user=os.getenv("DB_USER", "postgres"),
        password=os.getenv("DB_PASSWORD", ""),
    )


def query_spm_summary(conn):
    """查询 v_spm_summary_by_query_type 视图"""
    with conn.cursor() as cur:
        cur.execute("SELECT * FROM v_spm_summary_by_query_type ORDER BY avg_improvement_percent DESC NULLS LAST")
        columns = [desc[0] for desc in cur.description]
        return [dict(zip(columns, row)) for row in cur.fetchall()]


def query_high_impact_improvements(conn):
    """查询 v_spm_high_impact_improvements 视图"""
    with conn.cursor() as cur:
        cur.execute("SELECT * FROM v_spm_high_impact_improvements LIMIT 20")
        columns = [desc[0] for desc in cur.description]
        return [dict(zip(columns, row)) for row in cur.fetchall()]


def generate_markdown_report(data: list, high_impacts: list) -> str:
    """生成 Markdown 格式的报告"""
    
    report_time = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    
    md = f"""# Shadow Mode 性能对比日报

**生成时间**: {report_time}  
**统计周期**: 过去 7 天  

## 摘要

"""
    
    if data:
        total_improvements = sum(1 for d in data if d.get('avg_improvement_percent', 0) > 0)
        total_deteriorations = sum(1 for d in data if d.get('avg_improvement_percent', 0) < 0)
        
        md += f"- **优化项**: {total_improvements}/{len(data)} (有正收益)\n"
        md += f"- **退化项**: {total_deteriorations}/{len(data)} (有负收益)\n"
        md += f"- **平均改善率**: {sum(d.get('avg_improvement_percent', 0) or 0 for d in data)/len(data):.2f}%\n\n"
    else:
        md += "暂无数据记录。\n\n"
    
    md += "## 详细分析（按查询类型）\n\n"
    md += "| 查询类型 | 样本数 | 旧延迟 (ms) | 新延迟 (ms) | 改善率 | 准确性 | 完整性 |\n"
    md += "|---------|--------|------------|------------|-------|--------|--------|\n"
    
    for item in sorted(data, key=lambda x: x.get('avg_improvement_percent', 0), reverse=True):
        improvement = item.get('avg_improvement_percent', 0)
        if improvement is None:
            improvement_str = "N/A"
        elif improvement > 0:
            improvement_str = f"+{improvement:.1f}%"
        elif improvement < 0:
            improvement_str = f"{improvement:.1f}%"
        else:
            improvement_str = "0.0%"
        
        accuracy = f"{item.get('min_accuracy'):.3f}" if item.get('min_accuracy') else "N/A"
        completeness = f"{item.get('max_completeness'):.3f}" if item.get('max_completeness') else "N/A"
        
        md += f"| {item['query_type']} | {item['total_samples']} | {item['avg_old_latency_ms']:.1f} | {item['avg_new_latency_ms']:.1f} | {improvement_str} | {accuracy} | {completeness} |\n"
    
    md += "\n## 高影响改进 Top 10\n\n"
    
    if high_impacts:
        for i, impact in enumerate(high_impacts[:10], 1):
            md += f"### {i}. {impact['query_type']} ({impact['improvement_percent']:.1f}% 改善)\n\n"
            md += f"- **旧实现**: `{impact['old_implementation_name']}` → **新实现**: `{impact['new_implementation_name']}`\n"
            md += f"- **延迟**: {impact['old_latency_ms']}ms → {impact['new_latency_ms']}ms\n"
            md += f"- **准确性**: {impact.get('accuracy_score', 'N/A')} | **完整性**: N/A\n"
            md += f"- **验证人**: `{impact.get('validated_by', 'System')}` - {impact.get('operator_feedback', 'No comments')}\n\n"
    else:
        md += "暂无高影响改进记录。\n"
    
    md += "## 建议与结论\n\n"
    
    if data and any(d.get('avg_improvement_percent', 0) > 10 for d in data):
        md += "✅ **推荐部署**: 以下优化在多个查询类型中显示显著改进 (>10%):\n\n"
        for item in data:
            if item.get('avg_improvement_percent', 0) > 10 and item.get('min_accuracy', 0) >= 0.95:
                md += f"- `{item['query_type']}`: 平均改善 {item['avg_improvement_percent']:.1f}%\n"
    else:
        md += "⚠️ **待观察**: 当前无显著改进或退化的项目，建议继续收集数据。\n"
    
    return md


def generate_json_report(summary_data: list, high_impacts: list) -> dict:
    """生成 JSON 格式报告"""
    return {
        "generated_at": datetime.now().isoformat(),
        "summary_by_query_type": summary_data,
        "high_impact_improvements": high_impacts,
        "metrics": {
            "total_query_types": len(summary_data),
            "types_with_improvement": sum(1 for d in summary_data if d.get('avg_improvement_percent', 0) > 0),
            "types_with_degradation": sum(1 for d in summary_data if d.get('avg_improvement_percent', 0) < 0),
        }
    }


def main():
    parser = argparse.ArgumentParser(description="生成 Shadow Mode 性能对比日报")
    parser.add_argument("--output-dir", default="shadow_mode_reports", help="输出目录")
    parser.add_argument("--format", choices=["markdown", "json", "both"], default="both", help="输出格式")
    
    args = parser.parse_args()
    
    # 确保输出目录存在
    output_path = Path(args.output_dir)
    output_path.mkdir(parents=True, exist_ok=True)
    
    # 连接数据库并查询
    print("连接数据库...")
    conn = get_db_connection()
    
    try:
        summary_data = query_spm_summary(conn)
        high_impacts = query_high_impact_improvements(conn)
        
        print(f"获取到 {len(summary_data)} 种查询类型的统计数据")
        print(f"获取到 {len(high_impacts)} 条高影响改进记录")
        
        # 生成 Markdown 报告
        if args.format in ["markdown", "both"]:
            md_report = generate_markdown_report(summary_data, high_impacts)
            md_file = output_path / f"shadow_report_{datetime.now().strftime('%Y%m%d')}.md"
            md_file.write_text(md_report, encoding="utf-8")
            print(f"Markdown 报告已保存至: {md_file}")
        
        # 生成 JSON 报告
        if args.format in ["json", "both"]:
            json_report = generate_json_report(summary_data, high_impacts)
            json_file = output_path / f"shadow_report_{datetime.now().strftime('%Y%m%d')}.json"
            json_file.write_text(json.dumps(json_report, ensure_ascii=False, indent=2), encoding="utf-8")
            print(f"JSON 报告已保存至：{json_file}")
            
    finally:
        conn.close()


if __name__ == "__main__":
    main()

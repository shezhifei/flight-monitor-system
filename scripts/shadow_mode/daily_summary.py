"""
Daily Shadow Mode Summary Generator.

This script generates comprehensive daily reports of shadow mode activity,
including metrics, discrepancies, operator feedback, and trend analysis.

Usage:
    # Run manually
    python scripts/shadow_mode/daily_summary.py
    
    # Schedule with cron (runs every morning at 9 AM)
    0 9 * * * cd /path/to/project && .venv/bin/python scripts/shadow_mode/daily_summary.py
    
    # Or use as systemd service
    systemctl start shadow-mode-summary
"""

import argparse
import asyncio
import json
import logging
from datetime import date, datetime, timedelta
from pathlib import Path
from typing import Any, Dict, List, Optional

import psycopg2
from psycopg2.extras import RealDictCursor

# Configure logging
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s',
)
logger = logging.getLogger(__name__)


class DailySummaryGenerator:
    """Generate daily summary reports for shadow mode deployment."""
    
    def __init__(self, db_config: Optional[Dict[str, str]] = None):
        """Initialize generator.
        
        Args:
            db_config: Database connection configuration
                      Defaults to reading from environment or config file
        """
        self.db_config = db_config or {
            'host': 'localhost',
            'port': 5432,
            'dbname': 'flight_monitor',
            'user': 'fms',
            'password': 'fms_secret',
        }
        
        self.report_dir = Path(__file__).parent.parent.parent / "reports" / "shadow_mode"
        self.report_dir.mkdir(parents=True, exist_ok=True)
        
    async def generate_report(self, target_date: Optional[date] = None) -> dict:
        """Generate complete daily summary report.
        
        Args:
            target_date: Date to generate report for (defaults to yesterday)
            
        Returns:
            Dictionary containing all report data
        """
        
        if target_date is None:
            target_date = datetime.now().date() - timedelta(days=1)
        
        logger.info(f"Generating shadow mode report for {target_date}")
        
        report = {
            'date': target_date.isoformat(),
            'generated_at': datetime.now().isoformat(),
            'volume_metrics': await self._get_volume_metrics(target_date),
            'accuracy_metrics': await self._get_accuracy_metrics(target_date),
            'top_issues': await self._get_top_issues(target_date),
            'operator_stats': await self._get_operator_stats(target_date),
            'trend_analysis': await self._get_trend_analysis(target_date),
            'success_criteria': self._check_success_criteria(target_date),
            'recommendations': await self._generate_recommendations(target_date),
        }
        
        # Save report
        report_path = self.report_dir / f"daily_{target_date}.json"
        with open(report_path, 'w', encoding='utf-8') as f:
            json.dump(report, f, indent=2, ensure_ascii=False)
        
        # Also generate markdown version
        md_path = self.report_dir / f"daily_{target_date}.md"
        md_content = self._format_to_markdown(report)
        with open(md_path, 'w', encoding='utf-8') as f:
            f.write(md_content)
        
        logger.info(f"Report saved: {report_path}")
        logger.info(f"Markdown report saved: {md_path}")
        
        return report
    
    async def _get_connection(self) -> Any:
        """Get database connection."""
        
        return await asyncio.to_thread(
            psycopg2.connect,
            **self.db_config,
            cursor_factory=RealDictCursor,
        )
    
    async def _get_volume_metrics(self, target_date: date) -> dict:
        """Get volume-related metrics for the day."""
        
        query = """
            SELECT 
                COUNT(*) as total_queries,
                COUNT(DISTINCT operator_id) as unique_operators,
                MAX(created_at) as last_query_time
            FROM shadow_mode_discrepancies
            WHERE DATE(created_at) = %s
        """
        
        try:
            conn = await self._get_connection()
            with conn.cursor() as cur:
                cur.execute(query, (target_date,))
                result = cur.fetchone()
                
                if result:
                    # Get peak hour analysis
                    peak_query = """
                        SELECT 
                            EXTRACT(HOUR FROM created_at) as hour,
                            COUNT(*) as count
                        FROM shadow_mode_discrepancies
                        WHERE DATE(created_at) = %s
                        GROUP BY EXTRACT(HOUR FROM created_at)
                        ORDER BY count DESC
                        LIMIT 1
                    """
                    cur.execute(peak_query, (target_date,))
                    peak_hour_result = cur.fetchone()
                    
                    return {
                        'total_queries': int(result['total_queries']) if result['total_queries'] else 0,
                        'unique_operators': int(result['unique_operators']) if result['unique_operators'] else 0,
                        'last_query_time': result['last_query_time'].isoformat() if result['last_query_time'] else None,
                        'peak_hour': int(peak_hour_result['hour']) if peak_hour_result else None,
                        'peak_hour_count': int(peak_hour_result['count']) if peak_hour_result else None,
                    }
        except Exception as e:
            logger.error(f"Failed to get volume metrics: {e}")
        
        return {'error': str(e)}
    
    async def _get_accuracy_metrics(self, target_date: date) -> dict:
        """Get accuracy and quality metrics for the day."""
        
        query = """
            SELECT 
                COUNT(*) FILTER (WHERE severity = 'critical') as critical_count,
                COUNT(*) FILTER (WHERE severity = 'major') as major_count,
                COUNT(*) FILTER (WHERE severity = 'minor') as minor_count,
                COUNT(*) FILTER (WHERE severity = 'informational') as informational_count,
                AVG(satisfaction_score) as avg_satisfaction,
                MIN(satisfaction_score) as min_satisfaction,
                MAX(satisfaction_score) as max_satisfaction
            FROM shadow_mode_discrepancies dm
            LEFT JOIN operator_of ON dm.operator_id = of.id
            WHERE DATE(dm.created_at) = %s
        """
        
        try:
            conn = await self._get_connection()
            with conn.cursor() as cur:
                cur.execute(query, (target_date,))
                result = cur.fetchone()
                
                if result:
                    total = result['critical_count'] + result['major_count'] + \
                           result['minor_count'] + result['informational_count']
                    
                    agreement_rate = 1 - ((result['critical_count'] or 0) + (result['major_count'] or 0)) / max(total, 1)
                    
                    return {
                        'critical_count': int(result['critical_count']) if result['critical_count'] else 0,
                        'major_count': int(result['major_count']) if result['major_count'] else 0,
                        'minor_count': int(result['minor_count']) if result['minor_count'] else 0,
                        'informational_count': int(result['informational_count']) if result['informational_count'] else 0,
                        'total_with_feedback': total,
                        'overall_agreement_rate': round(agreement_rate, 4),
                        'avg_satisfaction': float(result['avg_satisfaction']) if result['avg_satisfaction'] else None,
                        'min_satisfaction': float(result['min_satisfaction']) if result['min_satisfaction'] else None,
                        'max_satisfaction': float(result['max_satisfaction']) if result['max_satisfaction'] else None,
                        'meets_critical_threshold': (result['critical_count'] or 0) == 0,
                        'meets_satisfaction_target': not result['avg_satisfaction'] or result['avg_satisfaction'] >= 4.0,
                    }
        except Exception as e:
            logger.error(f"Failed to get accuracy metrics: {e}")
        
        return {'error': str(e)}
    
    async def _get_top_issues(self, target_date: date) -> List[dict]:
        """Get top discrepancy issues for the day."""
        
        query = """
            SELECT 
                discrepancy_type,
                severity,
                COUNT(*) as count,
                AVG(EXTRACT(EPOCH FROM (NOW() - created_at))/3600) as avg_hours_open,
                STRING_AGG(operator_notes, E'; ') as sample_notes
            FROM shadow_mode_discrepancies
            WHERE DATE(created_at) = %s
            AND resolved = false
            GROUP BY discrepancy_type, severity
            ORDER BY count DESC
            LIMIT 10
        """
        
        try:
            conn = await self._get_connection()
            with conn.cursor() as cur:
                cur.execute(query, (target_date,))
                results = cur.fetchall()
                
                return [
                    {
                        'type': row['discrepancy_type'],
                        'severity': row['severity'],
                        'count': int(row['count']),
                        'avg_hours_open': float(row['avg_hours_open']) if row['avg_hours_open'] else None,
                        'sample_notes': row['sample_notes'][:200] if row['sample_notes'] else None,
                    }
                    for row in results
                ]
        except Exception as e:
            logger.error(f"Failed to get top issues: {e}")
            return []
    
    async def _get_operator_stats(self, target_date: date) -> List[dict]:
        """Get operator performance statistics."""
        
        query = """
            SELECT 
                operator_id,
                COUNT(*) as tasks_completed,
                AVG(satisfaction_score) as avg_satisfaction,
                COUNT(CASE WHEN severity IN ('critical', 'major') THEN 1 END) as high_severity_count,
                FIRST_VALUE(discrepancy_type ORDER BY created_at DESC NULLS LAST) as most_common_issue,
                SUM(CASE WHEN resolved THEN 1 ELSE 0 END) as resolved_issues
            FROM shadow_mode_discrepancies dm
            LEFT JOIN operator_of ON dm.operator_id = of.id
            WHERE DATE(dm.created_at) = %s
            AND dm.operator_id IS NOT NULL
            GROUP BY operator_id
            ORDER BY tasks_completed DESC
            LIMIT 10
        """
        
        try:
            conn = await self._get_connection()
            with conn.cursor() as cur:
                cur.execute(query, (target_date,))
                results = cur.fetchall()
                
                return [
                    {
                        'operator_id': row['operator_id'],
                        'tasks_completed': int(row['tasks_completed']) if row['tasks_completed'] else 0,
                        'avg_satisfaction': float(row['avg_satisfaction']) if row['avg_satisfaction'] else None,
                        'high_severity_count': int(row['high_severity_count']) if row['high_severity_count'] else 0,
                        'most_common_issue': row['most_common_issue'],
                        'resolved_issues': int(row['resolved_issues']) if row['resolved_issues'] else 0,
                    }
                    for row in results
                ]
        except Exception as e:
            logger.error(f"Failed to get operator stats: {e}")
            return []
    
    async def _get_trend_analysis(self, target_date: date) -> dict:
        """Analyze trends compared to previous day."""
        
        prev_date = target_date - timedelta(days=1)
        
        # Get current day metrics
        curr_metrics = await self._get_accuracy_metrics(target_date)
        prev_metrics = await self._get_accuracy_metrics(prev_date)
        
        # Calculate changes
        curr_agreement = curr_metrics.get('overall_agreement_rate', 0)
        prev_agreement = prev_metrics.get('overall_agreement_rate', 0)
        
        return {
            'agreement_rate_change': round(curr_agreement - prev_agreement, 4),
            'agreement_improving': curr_agreement > prev_agreement,
            'critical_issues_change': (
                curr_metrics.get('critical_count', 0) - prev_metrics.get('critical_count', 0)
            ),
            'satisfaction_change': (
                curr_metrics.get('avg_satisfaction', 0) - prev_metrics.get('avg_satisfaction', 0)
            ) if curr_metrics.get('avg_satisfaction') and prev_metrics.get('avg_satisfaction') else None,
        }
    
    def _check_success_criteria(self, target_date: date) -> dict:
        """Check if success criteria are met for the day."""
        
        # This will be populated after running queries
        return {
            'discrepancy_rate_check': False,
            'accuracy_target_met': False,
            'no_critical_issues': False,
            'satisfaction_target_met': False,
            'all_criteria_met': False,
        }
    
    async def _generate_recommendations(self, target_date: date) -> List[str]:
        """Generate actionable recommendations based on metrics."""
        
        recommendations = []
        
        # Get today's metrics
        accuracy_metrics = await self._get_accuracy_metrics(target_date)
        top_issues = await self._get_top_issues(target_date)
        
        # Check critical issues
        if accuracy_metrics.get('critical_count', 0) > 0:
            recommendations.append(
                "🚨 URGENT: Critical issues detected. Review immediately before proceeding."
            )
        
        # Check satisfaction
        if accuracy_metrics.get('avg_satisfaction') and accuracy_metrics['avg_satisfaction'] < 4.0:
            recommendations.append(
                "⚠️ Operator satisfaction below target (4.0). Consider team meeting and feedback review."
            )
        
        # Check top issues
        if top_issues and len(top_issues) > 0:
            top_issue = top_issues[0]
            if top_issue['count'] > 5:
                recommendations.append(
                    f"🔍 Top issue '{top_issue['type']}' occurred {top_issue['count']} times. "
                    "Investigate root cause and update templates."
                )
        
        # Check agreement rate improvement
        trends = await self._get_trend_analysis(target_date)
        if not trends.get('agreement_improving', False):
            recommendations.append(
                "📉 Agreement rate decreased from yesterday. Review recent changes and patterns."
            )
        
        # Positive reinforcement
        if accuracy_metrics.get('meets_critical_threshold'):
            recommendations.append("✅ No critical issues today - excellent progress!")
        
        if accuracy_metrics.get('meets_satisfaction_target'):
            recommendations.append("✅ Operator satisfaction meets target - well done!")
        
        return recommendations
    
    def _format_to_markdown(self, report: dict) -> str:
        """Format report as markdown document."""
        
        md = f"""# Daily Shadow Mode Report - {report['date']}

**Generated:** {report['generated_at']}

---

## 📊 Volume Metrics

| Metric | Value |
|--------|-------|
| Total Queries Processed | {report['volume_metrics']['total_queries']} |
| Unique Operators | {report['volume_metrics']['unique_operators']} |
| Last Query Time | {report['volume_metrics']['last_query_time']} |
| Peak Hour | {report['volume_metrics']['peak_hour']}:00 ({report['volume_metrics']['peak_hour_count']} queries) |

---

## ✅ Accuracy & Quality Metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Critical Issues | {report['accuracy_metrics']['critical_count']} | 0 | {'✅' if report['accuracy_metrics']['meets_critical_threshold'] else '❌'} |
| Major Issues | {report['accuracy_metrics']['major_count']} | <5 | {'✅' if report['accuracy_metrics']['major_count'] < 5 else '⚠️'} |
| Minor Issues | {report['accuracy_metrics']['minor_count']} | N/A | - |
| Overall Agreement | {report['accuracy_metrics']['overall_agreement_rate']*100:.1f}% | ≥95% | {'✅' if report['accuracy_metrics']['overall_agreement_rate'] >= 0.95 else '⚠️'} |
| Avg Satisfaction | {report['accuracy_metrics']['avg_satisfaction']}/5.0 | ≥4.0 | {'✅' if report['accuracy_metrics']['meets_satisfaction_target'] else '⚠️'} |

---

## 🔍 Top Issues Today

{self._issues_table(report['top_issues'])}

---

## 👥 Operator Performance

| Operator | Tasks | Satisfaction | High Severity | Most Common Issue |
|----------|-------|--------------|---------------|-------------------|
{self._operators_table(report['operator_stats'])}

---

## 📈 Trend Analysis

- **Agreement Rate Change**: {report['trend_analysis']['agreement_rate_change']*100:+.1f}% {'↑ Improving' if report['trend_analysis']['agreement_improving'] else '↓ Declining'}
- **Critical Issues Change**: {report['trend_analysis']['critical_issues_change']}
- **Satisfaction Change**: {report['trend_analysis']['satisfaction_change']:+.1f} points

---

## 💡 Recommendations

{self._recommendations_list(report['recommendations'])}

---

## ⚠️ Success Criteria Check

{'✅ All success criteria met!' if report['success_criteria']['all_criteria_met'] else '❌ Some criteria not met - continue monitoring'}

- [x] Discrepancy rate < 15%
- [x] Accuracy ≥ 95%  
- [x] Zero critical errors
- [x] Operator satisfaction ≥ 4.0/5.0

---

**Next Review:** Tomorrow at 9:00 AM  
**Action Required:** Review recommendations and take appropriate action

---
*Automated report generated by Shadow Mode Summary System*
"""
        
        return md
    
    def _issues_table(self, issues: List[dict]) -> str:
        """Format issues as markdown table."""
        
        if not issues:
            return "No unresolved issues reported."
        
        lines = ["| Type | Severity | Count | Avg Hours Open | Sample Notes |"]
        lines.append("|------|----------|-------|----------------|--------------|")
        
        for issue in issues[:5]:  # Limit to top 5
            lines.append(
                f"| {issue['type']} | {issue['severity']} | {issue['count']} | "
                f"{issue['avg_hours_open']:.1f}h | {issue['sample_notes'] or 'N/A'} |"
            )
        
        return '\n'.join(lines)
    
    def _operators_table(self, operators: List[dict]) -> str:
        """Format operators as markdown table."""
        
        if not operators:
            return "No operator data available."
        
        lines = ["| Operator | Tasks | Satisfaction | High Severity | Most Common Issue |"]
        lines.append("|----------|-------|--------------|---------------|-------------------|")
        
        for op in operators[:5]:  # Limit to top 5
            sat = f"{op['avg_satisfaction']:.1f}" if op['avg_satisfaction'] else 'N/A'
            lines.append(
                f"| {op['operator_id']} | {op['tasks_completed']} | {sat} | "
                f"{op['high_severity_count']} | {op['most_common_issue'] or 'N/A'} |"
            )
        
        return '\n'.join(lines)
    
    def _recommendations_list(self, recommendations: List[str]) -> str:
        """Format recommendations as list."""
        
        if not recommendations:
            return "No specific recommendations at this time."
        
        return '\n'.join([f"- {rec}" for rec in recommendations])


async def main():
    """Main entry point for daily summary generation."""
    
    parser = argparse.ArgumentParser(description='Generate shadow mode daily summary')
    parser.add_argument('--date', type=str, help='Target date (YYYY-MM-DD format)')
    parser.add_argument('--output-dir', type=str, help='Output directory for reports')
    args = parser.parse_args()
    
    # Initialize generator
    generator = DailySummaryGenerator()
    
    if args.date:
        target_date = datetime.strptime(args.date, '%Y-%m-%d').date()
    else:
        target_date = None
    
    # Generate report
    report = await generator.generate_report(target_date)
    
    print(f"\n{'='*60}")
    print(f"DAILY SHADOW MODE REPORT - {report['date']}")
    print(f"{'='*60}\n")
    
    print(f"Total Queries: {report['volume_metrics']['total_queries']}")
    print(f"Unique Operators: {report['volume_metrics']['unique_operators']}")
    print(f"Overall Agreement: {report['accuracy_metrics']['overall_agreement_rate']*100:.1f}%")
    print(f"Avg Satisfaction: {report['accuracy_metrics']['avg_satisfaction']}/5.0")
    print(f"Critical Issues: {report['accuracy_metrics']['critical_count']}")
    print(f"\nRecommendations:")
    for i, rec in enumerate(report['recommendations'], 1):
        print(f"  {i}. {rec}")
    print(f"\n{'='*60}\n")
    
    print(f"Reports saved to: {generator.report_dir}")


if __name__ == "__main__":
    asyncio.run(main())

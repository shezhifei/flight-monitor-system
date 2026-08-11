import { Space } from 'antd';

interface NavItem {
  key: string;
  label: string;
  href: string;
  description: string;
}

const NAV_ITEMS: NavItem[] = [
  {
    key: 'ai-config',
    label: '实体配置',
    href: '/frontend/ai_config_center.html',
    description: '管理实体、模型与工具权限',
  },
  {
    key: 'ai-monitor',
    label: '实时监控',
    href: '/frontend/ai_monitor.html',
    description: '查看执行事件、审批队列与运行指标',
  },
  {
    key: 'llm-eval',
    label: 'LLM 评测',
    href: '/frontend/llm_eval_lab.html',
    description: '创建评测任务并比较模型配置',
  },
  {
    key: 'nl-query',
    label: '自然语言查询',
    href: '/frontend/nl_query.html',
    description: '面向业务查询的对话式分析入口',
  },
];

interface AiPageNavigationProps {
  currentKey: string;
}

export function AiPageNavigation({ currentKey }: AiPageNavigationProps): JSX.Element {
  return (
    <div className="ai-page-nav ai-reveal ai-reveal-1">
      <Space wrap size={[6, 6]}>
        {NAV_ITEMS.map((item) => {
          const active = item.key === currentKey;
          return (
            <a
              key={item.key}
              href={item.href}
              className={`ai-page-nav__link${active ? ' is-active' : ''}`}
              aria-current={active ? 'page' : undefined}
            >
              <span className="ai-page-nav__label">{item.label}</span>
              <span className="ai-page-nav__desc">{item.description}</span>
            </a>
          );
        })}
      </Space>
    </div>
  );
}

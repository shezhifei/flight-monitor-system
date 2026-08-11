import { AiMonitorPage } from '@/features/ai-monitor/AiMonitorPage';
import { bootstrapEntry, renderBootstrapError } from '@/lib/shell/bootstrapEntry';
import '@/styles/global.css';

bootstrapEntry({ entryName: 'ai_monitor', render: () => <AiMonitorPage /> })
  .catch((error) => renderBootstrapError('ai_monitor', error));

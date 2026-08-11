import { LlmEvalLabPage } from '@/features/llm-eval/LlmEvalLabPage';
import { bootstrapEntry, renderBootstrapError } from '@/lib/shell/bootstrapEntry';
import '@/styles/global.css';

bootstrapEntry({ entryName: 'llm_eval_lab', render: () => <LlmEvalLabPage /> })
  .catch((error) => renderBootstrapError('llm_eval_lab', error));

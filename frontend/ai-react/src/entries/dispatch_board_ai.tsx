import { DispatchAiDrawerApp } from '@/features/dispatch-board/DispatchAiDrawerApp';
import { bootstrapEntry, renderBootstrapError } from '@/lib/shell/bootstrapEntry';
import '@/styles/global.css';

bootstrapEntry({ entryName: 'dispatch_board_ai', render: () => <DispatchAiDrawerApp /> })
  .catch((error) => renderBootstrapError('dispatch_board_ai', error));

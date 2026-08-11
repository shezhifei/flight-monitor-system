import '@/styles/main.css';
import '@/styles/dispatch-board.css';
import { mountDispatchBoardPage } from '@/pages/dispatch_board/mount';
import { bootstrapProtectedPage } from '@/shared/bootstrapProtectedPage';

await bootstrapProtectedPage(() => mountDispatchBoardPage());

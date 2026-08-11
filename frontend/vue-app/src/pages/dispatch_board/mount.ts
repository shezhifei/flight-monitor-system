import { createApp } from 'vue';
import DispatchBoard from './DispatchBoard.vue';
import '@/styles/main.css';
import '@/styles/dispatch-board.css';

/**
 * Mount the DispatchBoard page component into the app container.
 */
export function mountDispatchBoardPage(): void {
  const container = document.getElementById('app');
  
  if (!container) {
    console.error('App container not found');
    return;
  }

  const app = createApp(DispatchBoard);
  app.mount(container);
}

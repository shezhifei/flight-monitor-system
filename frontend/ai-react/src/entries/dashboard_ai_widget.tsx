import { AdminAiWidget } from '@/features/dashboard/AdminAiWidget';
import { bootstrapEntry, renderBootstrapError } from '@/lib/shell/bootstrapEntry';
import '@/styles/global.css';

bootstrapEntry({
  entryName: 'dashboard_ai_widget',
  render: () => <AdminAiWidget />,
  gate: (user) => {
    if (!user) return true;
    const isAdmin = Boolean((user as { is_admin?: boolean } | null)?.is_admin);
    const roles = Array.isArray((user as { roles?: unknown[] } | null)?.roles)
      ? (((user as { roles?: unknown[] }).roles as string[]) || [])
      : [];
    return isAdmin || roles.includes('admin');
  },
}).catch((error) => renderBootstrapError('dashboard_ai_widget', error));

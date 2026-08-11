import { NLQueryPage } from '@/features/nl-query/NLQueryPage';
import { bootstrapEntry, renderBootstrapError } from '@/lib/shell/bootstrapEntry';
import '@/styles/global.css';

bootstrapEntry({ entryName: 'nl_query', render: () => <NLQueryPage /> })
  .catch((error) => renderBootstrapError('nl_query', error));

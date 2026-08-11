/** Shared master-detail overview list item (派工规则 / 流程设计等). */
export interface AdminOverviewItem {
  id: string;
  title: string;
  /** Secondary line, e.g. code · category */
  meta?: string;
  /** Optional tertiary description */
  description?: string;
  /** When true and list showDelete is on, render delete control */
  deletable?: boolean;
  /**
   * Soft-deprecated (legacy flowable: is_active=false).
   * With actionMode="deprecate", shows restore instead of remove.
   */
  deprecated?: boolean;
}

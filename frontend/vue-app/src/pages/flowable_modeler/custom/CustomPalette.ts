/**
 * 业务节点自定义 palette：通知 / 表单任务 / 等待回执 等。
 */

type CreateService = {
  start: (event: unknown, shape: unknown, context?: unknown) => void;
};

type ElementFactory = {
  createShape: (attrs: { type: string }) => CustomShape;
};

type PaletteService = {
  registerProvider: (priority: number | CustomPalette, provider?: CustomPalette) => void;
};

type CustomShape = {
  businessNodePreset?: {
    nodeType: string;
    label?: string;
    defaultName?: string;
  };
  businessObject: {
    id?: string;
    name?: string;
  };
};

type PalettePreset = {
  nodeType: string;
  bpmnType: string;
  title: string;
  defaultName: string;
  className: string;
  imageUrl: string;
  fixedId?: string;
};

const ICON_NOTIFY =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#2563eb" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M18 8a6 6 0 10-12 0c0 7-3 7-3 7h18s-3 0-3-7"/>
      <path d="M13.73 21a2 2 0 01-3.46 0"/>
    </svg>`,
  );

const ICON_FORM =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#334155" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <rect x="4" y="3" width="16" height="18" rx="2"/>
      <path d="M8 8h8M8 12h8M8 16h5"/>
    </svg>`,
  );

const ICON_WAIT =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="#e6a23c">
      <path d="M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zm.5-13H11v6l5.25 3.15.75-1.23-4.5-2.67z"/>
    </svg>`,
  );

const ICON_DISPATCH =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#0f766e" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <path d="M9 11l3 3L22 4"/>
      <path d="M21 12v7a2 2 0 01-2 2H5a2 2 0 01-2-2V5a2 2 0 012-2h11"/>
    </svg>`,
  );

const ICON_COMPLETE =
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#7c3aed" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
      <circle cx="12" cy="12" r="9"/>
      <path d="M9 12l2 2 4-4"/>
    </svg>`,
  );

const PRESETS: PalettePreset[] = [
  {
    nodeType: 'notification',
    bpmnType: 'bpmn:UserTask',
    title: '创建通知节点（发送调度通知）',
    defaultName: '发送调度通知',
    className: 'entry-notification',
    imageUrl: ICON_NOTIFY,
  },
  {
    nodeType: 'form_task',
    bpmnType: 'bpmn:UserTask',
    title: '创建表单任务节点',
    defaultName: '填写处理表单',
    className: 'entry-form-task',
    imageUrl: ICON_FORM,
  },
  {
    nodeType: 'wait_receipts',
    bpmnType: 'bpmn:UserTask',
    title: '创建等待回执节点 (预设ID wait_receipts)',
    defaultName: '等待回执',
    className: 'entry-wait-receipts',
    imageUrl: ICON_WAIT,
    fixedId: 'wait_receipts',
  },
  {
    nodeType: 'dispatch_task',
    bpmnType: 'bpmn:UserTask',
    title: '创建派工任务节点',
    defaultName: '创建临时加单',
    className: 'entry-dispatch-task',
    imageUrl: ICON_DISPATCH,
  },
  {
    nodeType: 'business_case_action',
    bpmnType: 'bpmn:UserTask',
    title: '创建结束业务事项节点',
    defaultName: '结束业务事项',
    className: 'entry-case-action',
    imageUrl: ICON_COMPLETE,
  },
];

export default class CustomPalette {
  static $inject = ['palette', 'create', 'elementFactory'];

  private _create: CreateService;
  private _elementFactory: ElementFactory;

  constructor(palette: PaletteService, create: CreateService, elementFactory: ElementFactory) {
    palette.registerProvider(1200, this);
    this._create = create;
    this._elementFactory = elementFactory;
  }

  getPaletteEntries() {
    const entries: Record<string, unknown> = {};
    PRESETS.forEach((preset) => {
      entries[`create.${preset.nodeType}`] = {
        group: 'business',
        className: preset.className,
        title: preset.title,
        imageUrl: preset.imageUrl,
        action: {
          dragstart: (event: unknown) => this.startPreset(event, preset),
          click: (event: unknown) => this.startPreset(event, preset),
        },
      };
    });
    return entries;
  }

  private startPreset(event: unknown, preset: PalettePreset): void {
    const shape = this._elementFactory.createShape({ type: preset.bpmnType });
    shape.businessNodePreset = {
      nodeType: preset.nodeType,
      label: preset.title,
      defaultName: preset.defaultName,
    };
    shape.businessObject.name = preset.defaultName;
    if (preset.fixedId) {
      shape.businessObject.id = preset.fixedId;
    }
    this._create.start(event, shape);
  }
}

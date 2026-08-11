import { readonly, ref } from 'vue';

export type EmptyErrorType = 'empty' | 'error' | 'forbidden' | 'network';

export interface EmptyErrorState {
  key: string;
  type: EmptyErrorType;
  title: string;
  message: string;
  buttonLabel: string;
  pendingLabel: string;
  role: 'status' | 'alert';
  iconPath: string;
  retryPending: boolean;
  hasRetry: boolean;
}

export type EmptyErrorRetryHandler = () => unknown | Promise<unknown>;

type EmptyErrorMeta = Omit<EmptyErrorState, 'key' | 'type' | 'retryPending' | 'hasRetry'>;

const TYPE_META: Record<EmptyErrorType, EmptyErrorMeta> = {
  empty: {
    title: '暂无内容',
    message: '当前没有可展示的数据。',
    buttonLabel: '',
    pendingLabel: '',
    role: 'status',
    iconPath:
      'M7.37867966,3 C7.77650439,3 8.15803526,3.15803526 8.43933983,3.43933983 L11.7071068,6.70710678 C11.8946432,6.89464316 12.1489971,7 12.4142136,7 L19,7 C19.149679,7 19.2974379,7.00822125 19.4428497,7.02423682 C19.2349828,6.16929644 18.4757047,5.5288976 17.5622948,5.50095171 L17.5,5.5 L11.957,5.49986632 L10.45675,3.99986632 L17.5,4 C19.4329966,4 21,5.56700338 21,7.5 L21.0001204,7.53519846 C22.1956568,8.22683444 23,9.51948501 23,11 L23,17 C23,19.209139 21.209139,21 19,21 L5,21 C2.790861,21 1,19.209139 1,17 L1,7 C1,4.790861 2.790861,3 5,3 L7.37867966,3 Z M7.37867966,4.5 L5,4.5 C3.64269002,4.5 2.53801707,5.5816677 2.50096045,6.93002379 L2.5,17 C2.5,18.35731 3.5816677,19.4619829 4.93002379,19.4990396 L5,19.5 L19,19.5 C20.35731,19.5 21.4619829,18.4183323 21.4990396,17.0699762 L21.5,17 L21.5,11 C21.5,9.64269002 20.4183323,8.53801707 19.0699762,8.50096045 L12.4142136,8.5 C11.7787991,8.5 11.1683251,8.25809996 10.7059116,7.82529783 L10.6464466,7.76776695 L7.37867966,4.5 Z',
  },
  error: {
    title: '加载失败',
    message: '数据暂时不可用，请稍后重试。',
    buttonLabel: '重试',
    pendingLabel: '重试中...',
    role: 'alert',
    iconPath:
      'M12,0.999983593 L12.1815564,1.00145453 C18.0835105,1.09714557 22.8442179,5.83597302 22.9962538,11.7184846 L23,12.0086951 L22.9985643,12.1815573 C22.9028731,18.083502 18.1640381,22.8442018 12.281517,22.9962374 L11.9913061,22.9999836 L11.8184436,22.9985479 C5.91648945,22.9028568 1.15578206,18.1640294 1.00374622,12.2815178 L1,11.9913073 L1.00143573,11.8184451 C1.09712692,5.91650039 5.83596195,1.15576653 11.710203,1.00372984 L12,0.999983593 Z M11.994,2.499 L11.7279371,2.50380847 C6.7569636,2.64374211 2.72991179,6.60611289 2.50960807,11.5597393 L2.50138399,11.8309031 L2.5,11.9913073 C2.5,17.0940492 6.51492881,21.2660064 11.5596789,21.4903735 L11.8309016,21.4985996 L11.9913061,21.4999836 C17.0940585,21.4999836 21.2660225,17.4850612 21.4903899,12.4403214 L21.498616,12.1690993 L21.5,12.0086951 C21.5,6.90595322 17.4850712,2.73399601 12.4406109,2.50963127 L12.169404,2.50140531 L11.994,2.499 Z M12,15.5 C12.6903125,15.5 13.25,16.0596875 13.25,16.75 C13.25,17.4403125 12.6903125,18 12,18 C11.3096875,18 10.75,17.4403125 10.75,16.75 C10.75,16.0596875 11.3096875,15.5 12,15.5 Z M12,6 C12.6414582,6 13.1614628,6.5200046 13.1614628,7.16146279 C13.1614628,7.18988207 13.1604197,7.21829177 13.1583357,7.24663453 L12.6708322,13.8766827 C12.6449936,14.2280877 12.3523536,14.5 12,14.5 C11.6476464,14.5 11.3550064,14.2280877 11.3291678,13.8766827 L10.8416643,7.24663453 C10.7946252,6.60690339 11.2750971,6.05016615 11.9148283,6.00312709 C11.943171,6.00104307 11.9715807,6 12,6 Z',
  },
  forbidden: {
    title: '无权访问',
    message: '您当前没有查看此内容的权限。',
    buttonLabel: '',
    pendingLabel: '',
    role: 'alert',
    iconPath:
      'M12,1 C18.0751322,1 23,5.92486775 23,12 C23,14.9143925 21.8666105,17.5640678 20.0165552,19.532302 L19.7651925,19.7911352 C17.7756182,21.7741259 15.0309682,23 12,23 C5.92486775,23 1,18.0751322 1,12 C1,9.08560747 2.13338952,6.43593224 3.98344482,4.46769805 L4.23480753,4.20886478 C6.22438177,2.2258741 8.96903176,1 12,1 Z M2.5,12 C2.5,17.2467051 6.75329488,21.5 12,21.5 C14.3534071,21.5 16.5069434,20.6442512 18.1664151,19.2269473 L4.77305273,5.83358488 C3.35574883,7.49305656 2.5,9.64659286 2.5,12 Z M12,2.5 C9.64659286,2.5 7.49305656,3.35574883 5.83358488,4.77305273 L19.2269473,18.1664151 C20.6442512,16.5069434 21.5,14.3534071 21.5,12 C21.5,6.75329488 17.2467051,2.5 12,2.5 Z',
  },
  network: {
    title: '网络异常',
    message: '网络连接不可用，请检查网络后重试。',
    buttonLabel: '重新连接',
    pendingLabel: '连接中...',
    role: 'alert',
    iconPath:
      'M16.447592,14.0940386 C16.8470122,14.0940386 17.1735072,14.4062686 17.1963188,14.7999704 L17.197592,14.8440386 L17.197592,18.5940386 C17.197592,21.3554623 14.9590157,23.5940386 12.197592,23.5940386 C9.46493306,23.5940386 7.24428571,21.4018564 7.19831874,18.6801609 L7.19759198,14.8440386 C7.19759198,14.429825 7.53337841,14.0940386 7.94759198,14.0940386 C8.3470122,14.0940386 8.6735072,14.4062686 8.6963188,14.7999704 L8.69759198,14.8440386 L8.69759198,18.5940386 C8.69759198,20.5270352 10.2645954,22.0940386 12.197592,22.0940386 C14.104467,22.0940386 15.6551737,20.5691005 15.6967365,18.6721998 L15.697592,14.8440386 C15.697592,14.429825 16.0333784,14.0940386 16.447592,14.0940386 Z M12.197592,6.84403858 C12.6118055,6.84403858 12.947592,7.17982502 12.947592,7.59403858 L12.947592,16.0940386 C12.947592,16.5082521 12.6118055,16.8440386 12.197592,16.8440386 C11.7833784,16.8440386 11.447592,16.5082521 11.447592,16.0940386 L11.447592,7.59403858 C11.447592,7.17982502 11.7833784,6.84403858 12.197592,6.84403858 Z M12.197592,0.0940385849 C14.9302509,0.0940385849 17.1508982,2.28622073 17.1968652,5.00791623 L17.197592,8.84403858 C17.197592,9.25825215 16.8618055,9.59403858 16.447592,9.59403858 C16.0481718,9.59403858 15.7216768,9.28180859 15.6988651,8.88810678 L15.697592,8.84403858 L15.697592,5.09403858 C15.697592,3.16104196 14.1305886,1.59403858 12.197592,1.59403858 C10.2907169,1.59403858 8.74001025,3.11897668 8.69844749,5.01587733 L8.69759198,5.09403858 L8.69759198,8.84403858 C8.69759198,9.25825215 8.36180554,9.59403858 7.94759198,9.59403858 C7.54817175,9.59403858 7.22167676,9.28180859 7.19886515,8.88810678 L7.19759198,8.84403858 L7.19759198,5.09403858 C7.19759198,2.33261484 9.43616823,0.0940385849 12.197592,0.0940385849 Z',
  },
};

const states = ref<Record<string, EmptyErrorState>>({});
const retryHandlers = new Map<string, EmptyErrorRetryHandler>();

function normalizeType(type: EmptyErrorType | string): EmptyErrorType {
  const normalized = String(type || '').trim().toLowerCase();
  if (normalized === 'empty' || normalized === 'error' || normalized === 'forbidden' || normalized === 'network') {
    return normalized;
  }
  return 'empty';
}

function showEmptyError(
  key: string,
  type: EmptyErrorType | string,
  message?: string,
  onRetry?: EmptyErrorRetryHandler,
): EmptyErrorState {
  const normalizedKey = String(key || '').trim();
  if (!normalizedKey) {
    throw new Error('empty/error state key is required');
  }

  const normalizedType = normalizeType(type);
  const meta = TYPE_META[normalizedType];
  const state: EmptyErrorState = {
    key: normalizedKey,
    type: normalizedType,
    title: meta.title,
    message: String(message ?? '').trim() || meta.message,
    buttonLabel: meta.buttonLabel,
    pendingLabel: meta.pendingLabel,
    role: meta.role,
    iconPath: meta.iconPath,
    retryPending: false,
    hasRetry: typeof onRetry === 'function',
  };

  if (onRetry) {
    retryHandlers.set(normalizedKey, onRetry);
  } else {
    retryHandlers.delete(normalizedKey);
  }

  states.value = {
    ...states.value,
    [normalizedKey]: state,
  };

  return state;
}

function clearEmptyError(key: string): boolean {
  const normalizedKey = String(key || '').trim();
  if (!normalizedKey || !states.value[normalizedKey]) {
    return false;
  }

  retryHandlers.delete(normalizedKey);
  const nextStates = { ...states.value };
  delete nextStates[normalizedKey];
  states.value = nextStates;
  return true;
}

async function retryEmptyError(key: string): Promise<boolean> {
  const normalizedKey = String(key || '').trim();
  const handler = retryHandlers.get(normalizedKey);
  const state = states.value[normalizedKey];

  if (!handler || !state || state.retryPending) {
    return false;
  }

  states.value = {
    ...states.value,
    [normalizedKey]: {
      ...state,
      retryPending: true,
    },
  };

  try {
    await Promise.resolve(handler());
    return true;
  } finally {
    const nextState = states.value[normalizedKey];
    if (nextState) {
      states.value = {
        ...states.value,
        [normalizedKey]: {
          ...nextState,
          retryPending: false,
        },
      };
    }
  }
}

function clearAllEmptyErrors(): void {
  retryHandlers.clear();
  states.value = {};
}

export function useEmptyError() {
  return {
    states: readonly(states),
    showEmptyError,
    show: showEmptyError,
    clearEmptyError,
    clear: clearEmptyError,
    retryEmptyError,
    clearAllEmptyErrors,
    meta: TYPE_META,
  };
}

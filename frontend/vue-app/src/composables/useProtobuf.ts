import { computed, readonly, shallowRef, ref } from 'vue';
import type { IConversionOptions, Type } from 'protobufjs';
import { createRoot, loadProto } from '@/lib/protobuf';

type ProtobufRoot = Awaited<ReturnType<typeof loadProto>>;

export interface ProtobufLoadOptions {
  root?: ProtobufRoot;
}

const DEFAULT_CONVERSION_OPTIONS: IConversionOptions = {
  longs: Number,
  enums: String,
  defaults: false,
  arrays: true,
  objects: true,
};

export function useProtobuf(initialSource?: string | string[]) {
  const root = shallowRef<ProtobufRoot | null>(null);
  const loading = ref(false);
  const error = ref<Error | null>(null);

  async function load(source: string | string[] = initialSource ?? '', options: ProtobufLoadOptions = {}): Promise<ProtobufRoot> {
    const targetSource = Array.isArray(source) ? source : String(source).trim();
    const hasSource = Array.isArray(targetSource) ? targetSource.length > 0 : Boolean(targetSource);

    if (!hasSource) {
      throw new Error('proto source is required');
    }

    loading.value = true;
    error.value = null;

    try {
      const nextRoot = await loadProto(targetSource, options.root ?? createRoot());
      root.value = nextRoot;
      return nextRoot;
    } catch (loadError) {
      const normalizedError = loadError instanceof Error ? loadError : new Error(String(loadError));
      error.value = normalizedError;
      throw normalizedError;
    } finally {
      loading.value = false;
    }
  }

  function requireRoot(): ProtobufRoot {
    if (!root.value) {
      throw new Error('protobuf schema has not been loaded');
    }
    return root.value;
  }

  function lookupType(typeName: string): Type {
    return requireRoot().lookupType(typeName) as Type;
  }

  async function decode<T extends object = Record<string, unknown>>(
    typeName: string,
    payload: ArrayBuffer | Uint8Array,
    conversionOptions: IConversionOptions = DEFAULT_CONVERSION_OPTIONS,
  ): Promise<T> {
    const messageType = lookupType(typeName);
    const buffer = payload instanceof Uint8Array ? payload : new Uint8Array(payload);
    const decoded = messageType.decode(buffer);
    return messageType.toObject(decoded, conversionOptions) as T;
  }

  async function decodeResponse<T extends object = Record<string, unknown>>(
    typeName: string,
    response: Response,
    conversionOptions: IConversionOptions = DEFAULT_CONVERSION_OPTIONS,
  ): Promise<T> {
    const buffer = await response.arrayBuffer();
    return decode<T>(typeName, buffer, conversionOptions);
  }

  return {
    root: readonly(root),
    loading: readonly(loading),
    error: readonly(error),
    isReady: computed(() => root.value !== null),
    load,
    lookupType,
    decode,
    decodeResponse,
  };
}

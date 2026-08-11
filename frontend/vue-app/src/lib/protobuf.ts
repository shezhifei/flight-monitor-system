import * as protobuf from 'protobufjs';

export { protobuf };
export type { Root } from 'protobufjs';

export function createRoot(): protobuf.Root {
  return new protobuf.Root();
}

export async function loadProto(
  source: string | string[],
  root: protobuf.Root = createRoot(),
): Promise<protobuf.Root> {
  return protobuf.load(source, root);
}

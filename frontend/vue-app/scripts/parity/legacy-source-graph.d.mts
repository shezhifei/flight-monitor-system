export interface LegacyAssetReference {
  kind: 'script' | 'stylesheet' | 'asset' | 'external' | 'dynamic';
  reference: string;
  archivePath: string | null;
  exists: boolean;
  sha256: string | null;
}

export interface LegacySourceGraph {
  html: string;
  htmlSha256: string;
  scripts: LegacyAssetReference[];
  stylesheets: LegacyAssetReference[];
  assets: LegacyAssetReference[];
  sourceHash: string;
  sourceFiles: Array<{ path: string; sha256: string }>;
}

export function extractLegacySourceContract(
  legacyRoot: string,
  htmlArchivePath: string,
): Promise<LegacySourceGraph>;

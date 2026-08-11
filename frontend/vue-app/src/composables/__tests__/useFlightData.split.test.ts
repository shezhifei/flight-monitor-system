import { describe, it, expect } from 'vitest';
import useFlightDataRaw from '../useFlightData?raw';
import useFlightCrudRaw from '../useFlightCrud?raw';
import useFlightSyncRaw from '../useFlightSync?raw';
import useFlightFilterRaw from '../useFlightFilter?raw';
import useFlightFieldRaw from '../useFlightField?raw';
import useFlightFetchRaw from '../useFlightFetch?raw';

describe('useFlightData split', () => {
  it('main file should be under 500 lines', () => {
    const lines = useFlightDataRaw.split('\n').length;
    expect(lines).toBeLessThan(500);
  });

  it('sub-composables exist', () => {
    expect(useFlightCrudRaw).toBeTruthy();
    expect(useFlightSyncRaw).toBeTruthy();
    expect(useFlightFilterRaw).toBeTruthy();
    expect(useFlightFieldRaw).toBeTruthy();
    expect(useFlightFetchRaw).toBeTruthy();
  });
});

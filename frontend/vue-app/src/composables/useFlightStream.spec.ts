import { describe, it, expect, vi } from 'vitest';
import { buildDebouncedScheduler } from './useFlightStream';

describe('scheduleFullSync debounce', () => {
  it('does not schedule another full sync within min interval', () => {
    vi.useFakeTimers();
    let callCount = 0;
    const schedule = buildDebouncedScheduler(() => {
      callCount += 1;
    }, 5000);
    schedule();
    schedule();
    vi.advanceTimersByTime(0);
    expect(callCount).toBe(1);
    vi.advanceTimersByTime(5000);
    schedule();
    vi.advanceTimersByTime(0);
    expect(callCount).toBe(2);
    vi.useRealTimers();
  });

  it('defers execution when called within min interval', () => {
    vi.useFakeTimers();
    let callCount = 0;
    const schedule = buildDebouncedScheduler(() => {
      callCount += 1;
    }, 5000);

    schedule();
    vi.advanceTimersByTime(0);
    expect(callCount).toBe(1);

    vi.advanceTimersByTime(3000);
    schedule();
    vi.advanceTimersByTime(0);
    expect(callCount).toBe(1);

    vi.advanceTimersByTime(2000);
    expect(callCount).toBe(2);
    vi.useRealTimers();
  });
});

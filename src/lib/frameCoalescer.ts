/** Coalesce frequent events into the next animation frame; cleared callbacks cannot restore stale
 * state.
 */
export class FrameCoalescer<T> {
  private pending: T | null = null;
  private frame: number | null = null;
  private readonly schedule: (callback: () => void) => number;
  private readonly cancel: (handle: number) => void;
  private readonly consume: (value: T | null) => void;
  constructor(
    schedule: (callback: () => void) => number,
    cancel: (handle: number) => void,
    consume: (value: T | null) => void,
  ) {
    this.schedule = schedule;
    this.cancel = cancel;
    this.consume = consume;
  }
  push(value: T) {
    this.pending = value;
    if (this.frame != null) return;
    this.frame = this.schedule(() => {
      this.frame = null;
      const next = this.pending;
      this.pending = null;
      if (next != null) this.consume(next);
    });
  }
  clear() {
    this.pending = null;
    if (this.frame != null) this.cancel(this.frame);
    this.frame = null;
    this.consume(null);
  }
  dispose() {
    this.pending = null;
    if (this.frame != null) this.cancel(this.frame);
    this.frame = null;
  }
}

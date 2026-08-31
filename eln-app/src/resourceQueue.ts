/** FIFO permits. A permit covers the full resource lifetime, not just download. */
export class ResourceQueue {
  private active = 0;
  private waiting: (() => void)[] = [];
  private readonly limit: number;
  constructor(limit: number) {
    if (!Number.isInteger(limit) || limit < 1)
      throw new Error("Invalid resource limit");
    this.limit = limit;
  }

  acquire(signal: AbortSignal): Promise<() => void> {
    signal.throwIfAborted();
    return new Promise((resolve, reject) => {
      const abort = () => {
        this.waiting = this.waiting.filter((item) => item !== grant);
        reject(signal.reason);
      };
      const grant = () => {
        signal.removeEventListener("abort", abort);
        this.active++;
        let released = false;
        resolve(() => {
          if (released) return;
          released = true;
          this.active--;
          this.waiting.shift()?.();
        });
      };
      if (this.active < this.limit) grant();
      else {
        signal.addEventListener("abort", abort, { once: true });
        this.waiting.push(grant);
      }
    });
  }
}

import type { FoundryLocalEvent } from './types.js';

type Handler = (event: FoundryLocalEvent) => void;

class EventBus {
  private handlers: Handler[] = [];

  subscribe(fn: Handler): () => void {
    this.handlers.push(fn);
    return () => {
      this.handlers = this.handlers.filter(h => h !== fn);
    };
  }

  publish(event: FoundryLocalEvent): void {
    for (const h of this.handlers) {
      try {
        h(event);
      } catch (err) {
        console.error('[foundryLocalBus] handler error', err);
      }
    }
  }
}

export const foundryLocalBus = new EventBus();

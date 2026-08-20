export interface HydrateSubscribedStateOptions<T> {
  subscriptionsReady: Promise<unknown>;
  readSnapshot: () => Promise<T>;
  currentRevision: () => number;
  applySnapshot: (value: T) => void;
  isActive: () => boolean;
}

export async function hydrateSubscribedState<T>(
  options: HydrateSubscribedStateOptions<T>,
): Promise<void> {
  await options.subscriptionsReady;
  if (!options.isActive()) return;

  const revisionAtRead = options.currentRevision();
  const snapshot = await options.readSnapshot();

  if (
    options.isActive() &&
    options.currentRevision() === revisionAtRead
  ) {
    options.applySnapshot(snapshot);
  }
}

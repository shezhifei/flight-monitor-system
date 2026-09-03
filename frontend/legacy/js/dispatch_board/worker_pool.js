(function (window) {
    'use strict';

    const DEFAULT_MAX_WORKERS = 2;
    const DEFAULT_TIMEOUT_MS = 30000;

    function normalizePositiveInteger(value, fallback) {
        const numeric = Number(value);
        if (!Number.isFinite(numeric) || numeric <= 0) {
            return fallback;
        }
        return Math.max(1, Math.floor(numeric));
    }

    class DispatchBoardWorkerPool {
        constructor(options) {
            const settings = options && typeof options === 'object' ? options : {};
            const workerUrl = String(settings.workerUrl || '').trim();
            if (!workerUrl) {
                throw new Error('workerUrl is required');
            }

            this.workerUrl = workerUrl;
            this.workerOptions = settings.workerOptions && typeof settings.workerOptions === 'object'
                ? { ...settings.workerOptions }
                : { type: 'module' };
            this.maxWorkers = normalizePositiveInteger(settings.maxWorkers, DEFAULT_MAX_WORKERS);
            this.defaultTimeoutMs = normalizePositiveInteger(settings.timeoutMs, DEFAULT_TIMEOUT_MS);
            this.workers = [];
            this.queue = [];
            this.nextJobId = 1;
            this.disposed = false;
        }

        run(payload, options) {
            if (this.disposed) {
                return Promise.reject(new Error('worker pool has been disposed'));
            }

            const settings = options && typeof options === 'object' ? options : {};
            const timeoutMs = normalizePositiveInteger(settings.timeoutMs, this.defaultTimeoutMs);

            return new Promise((resolve, reject) => {
                this.queue.push({
                    id: this.nextJobId++,
                    payload,
                    timeoutMs,
                    resolve,
                    reject
                });
                this.pumpQueue();
            });
        }

        pumpQueue() {
            if (this.disposed) {
                return;
            }

            while (this.queue.length > 0) {
                const entry = this.getIdleWorkerEntry() || this.createWorkerEntry();
                if (!entry) {
                    return;
                }
                const job = this.queue.shift();
                if (!job) {
                    return;
                }
                this.assignJob(entry, job);
            }
        }

        getIdleWorkerEntry() {
            return this.workers.find((entry) => !entry.busy) || null;
        }

        createWorkerEntry() {
            if (this.workers.length >= this.maxWorkers) {
                return null;
            }

            const worker = new Worker(this.workerUrl, this.workerOptions);
            const entry = {
                worker,
                busy: false,
                currentJob: null,
                timeoutHandle: 0,
                messageHandler: null,
                errorHandler: null
            };
            this.workers.push(entry);
            return entry;
        }

        assignJob(entry, job) {
            entry.busy = true;
            entry.currentJob = job;

            const finalize = () => {
                if (entry.timeoutHandle) {
                    window.clearTimeout(entry.timeoutHandle);
                    entry.timeoutHandle = 0;
                }
                if (entry.messageHandler) {
                    entry.worker.removeEventListener('message', entry.messageHandler);
                    entry.messageHandler = null;
                }
                if (entry.errorHandler) {
                    entry.worker.removeEventListener('error', entry.errorHandler);
                    entry.errorHandler = null;
                }
                entry.busy = false;
                entry.currentJob = null;
            };

            const recycleWorker = () => {
                try {
                    entry.worker.terminate();
                } catch (_) {
                    // ignore terminate errors
                }
                const index = this.workers.indexOf(entry);
                if (index >= 0) {
                    this.workers.splice(index, 1);
                }
            };

            const settle = (callback, options) => {
                const settings = options && typeof options === 'object' ? options : {};
                finalize();
                if (settings.recycleWorker) {
                    recycleWorker();
                }
                callback();
                this.pumpQueue();
            };

            entry.messageHandler = (event) => {
                const response = event?.data || {};
                if (!response.ok) {
                    settle(() => job.reject(new Error(response.error || 'worker solve failed')));
                    return;
                }
                settle(() => job.resolve(response.payload || {}));
            };

            entry.errorHandler = (event) => {
                settle(() => job.reject(new Error(event?.message || 'worker execution failed')), {
                    recycleWorker: true
                });
            };

            entry.worker.addEventListener('message', entry.messageHandler);
            entry.worker.addEventListener('error', entry.errorHandler);
            entry.timeoutHandle = window.setTimeout(() => {
                settle(() => job.reject(new Error('wasm solver timeout')), {
                    recycleWorker: true
                });
            }, job.timeoutMs);

            try {
                entry.worker.postMessage(job.payload);
            } catch (error) {
                settle(() => job.reject(error instanceof Error ? error : new Error(String(error || 'worker dispatch failed'))), {
                    recycleWorker: true
                });
            }
        }

        getStats() {
            return {
                maxWorkers: this.maxWorkers,
                totalWorkers: this.workers.length,
                activeWorkers: this.workers.filter((entry) => entry.busy).length,
                queuedJobs: this.queue.length,
                disposed: this.disposed
            };
        }

        dispose() {
            if (this.disposed) {
                return;
            }
            this.disposed = true;
            const pendingJobs = this.queue.splice(0, this.queue.length);
            pendingJobs.forEach((job) => {
                job.reject(new Error('worker pool has been disposed'));
            });
            this.workers.splice(0, this.workers.length).forEach((entry) => {
                if (entry.timeoutHandle) {
                    window.clearTimeout(entry.timeoutHandle);
                }
                if (entry.messageHandler) {
                    entry.worker.removeEventListener('message', entry.messageHandler);
                }
                if (entry.errorHandler) {
                    entry.worker.removeEventListener('error', entry.errorHandler);
                }
                if (entry.currentJob) {
                    entry.currentJob.reject(new Error('worker pool has been disposed'));
                }
                try {
                    entry.worker.terminate();
                } catch (_) {
                    // ignore terminate errors
                }
            });
        }
    }

    const sharedPools = new Map();

    function buildSharedPoolKey(options) {
        const settings = options && typeof options === 'object' ? options : {};
        return JSON.stringify({
            workerUrl: String(settings.workerUrl || '').trim(),
            workerType: String(settings.workerOptions?.type || 'module').trim() || 'module',
            maxWorkers: normalizePositiveInteger(settings.maxWorkers, DEFAULT_MAX_WORKERS),
            timeoutMs: normalizePositiveInteger(settings.timeoutMs, DEFAULT_TIMEOUT_MS)
        });
    }

    function getSharedPool(options) {
        const key = buildSharedPoolKey(options);
        if (!sharedPools.has(key)) {
            sharedPools.set(key, new DispatchBoardWorkerPool(options));
        }
        return sharedPools.get(key);
    }

    function disposeSharedPools() {
        sharedPools.forEach((pool) => pool.dispose());
        sharedPools.clear();
    }

    window.DispatchBoardWorkerPool = {
        createPool(options) {
            return new DispatchBoardWorkerPool(options);
        },
        getSharedPool,
        disposeSharedPools
    };
}(window));

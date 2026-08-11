import { useAuth } from './useAuth';

export type ApiParseMode = 'auto' | 'json' | 'text' | 'blob' | 'arrayBuffer' | 'response';

export interface ApiRequestOptions extends RequestInit {
  parseAs?: ApiParseMode;
}

export interface ApiResult<T> {
  ok: boolean;
  status: number;
  data: T | null;
  response: Response;
}

async function parseResponseBody<T>(response: Response, parseAs: ApiParseMode): Promise<T | null> {
  if (parseAs === 'response') {
    return response as T;
  }

  if (response.status === 204) {
    return null;
  }

  if (parseAs === 'blob') {
    return (await response.blob()) as T;
  }

  if (parseAs === 'arrayBuffer') {
    return (await response.arrayBuffer()) as T;
  }

  if (parseAs === 'text') {
    return (await response.text()) as T;
  }

  const contentType = String(response.headers.get('content-type') || '').toLowerCase();
  if (parseAs === 'json' || contentType.includes('application/json')) {
    return (await response.json()) as T;
  }

  return (await response.text()) as T;
}

function buildJsonRequestOptions(options: ApiRequestOptions, body?: unknown): ApiRequestOptions {
  if (body === undefined) {
    return options;
  }

  return {
    ...options,
    body: typeof body === 'string' || body instanceof FormData ? body : JSON.stringify(body),
  };
}

export type RequestInterceptor = (request: Request) => Request | Promise<Request>;
export type ResponseInterceptor = (response: Response) => Response | Promise<Response>;

export function useApi() {
  const auth = useAuth();
  const _requestInterceptors: RequestInterceptor[] = [];
  const _responseInterceptors: ResponseInterceptor[] = [];

  function addRequestInterceptor(fn: RequestInterceptor): () => void {
    _requestInterceptors.push(fn);
    return () => {
      const idx = _requestInterceptors.indexOf(fn);
      if (idx >= 0) _requestInterceptors.splice(idx, 1);
    };
  }

  function addResponseInterceptor(fn: ResponseInterceptor): () => void {
    _responseInterceptors.push(fn);
    return () => {
      const idx = _responseInterceptors.indexOf(fn);
      if (idx >= 0) _responseInterceptors.splice(idx, 1);
    };
  }

  async function raw(input: RequestInfo | URL, options: ApiRequestOptions = {}): Promise<Response> {
    if (_requestInterceptors.length === 0 && _responseInterceptors.length === 0) {
      return auth.fetch(input, options);
    }
    const url = typeof input === 'string' && !input.startsWith('http')
      ? new URL(input, window.location.origin).href
      : input;
    let request = new Request(url, options);
    for (const interceptor of _requestInterceptors) {
      request = await interceptor(request);
    }
    let response = await auth.fetch(request.url, {
      method: request.method,
      headers: request.headers,
      body: request.body,
    });
    for (const interceptor of _responseInterceptors) {
      response = await interceptor(response);
    }
    return response;
  }

  async function request<T>(input: RequestInfo | URL, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    const response = await raw(input, options);
    const parseAs = options.parseAs ?? 'auto';
    let data: T | null = null;

    if (response.ok || response.status === 204) {
      data = await parseResponseBody<T>(response, parseAs);
    } else {
      try {
        data = await parseResponseBody<T>(response, parseAs);
      } catch {
        data = null;
      }
    }

    return {
      ok: response.ok,
      status: response.status,
      data,
      response,
    };
  }

  async function get<T>(input: RequestInfo | URL, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    return request<T>(input, {
      ...options,
      method: options.method ?? 'GET',
    });
  }

  async function post<T>(input: RequestInfo | URL, body?: unknown, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    return request<T>(input, {
      ...buildJsonRequestOptions(options, body),
      method: options.method ?? 'POST',
    });
  }

  async function put<T>(input: RequestInfo | URL, body?: unknown, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    return request<T>(input, {
      ...buildJsonRequestOptions(options, body),
      method: options.method ?? 'PUT',
    });
  }

  async function patch<T>(input: RequestInfo | URL, body?: unknown, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    return request<T>(input, {
      ...buildJsonRequestOptions(options, body),
      method: options.method ?? 'PATCH',
    });
  }

  async function del<T>(input: RequestInfo | URL, options: ApiRequestOptions = {}): Promise<ApiResult<T>> {
    return request<T>(input, {
      ...options,
      method: options.method ?? 'DELETE',
    });
  }

  return {
    raw,
    request,
    get,
    post,
    put,
    patch,
    delete: del,
    addRequestInterceptor,
    addResponseInterceptor,
  };
}

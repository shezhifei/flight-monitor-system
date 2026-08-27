import { computed, ref } from 'vue';
import { useApi } from './useApi';
import { useAuth } from './useAuth';
import { useToast } from './useToast';

export interface OccupiableSeat {
  id: string;
  username: string;
  display_name: string;
  department?: string | null;
  current_occupant_user_id?: string | null;
}

interface SeatListResponse {
  items?: OccupiableSeat[];
  total?: number;
}

interface TokenResponse {
  access_token?: string;
  refresh_token?: string;
  expires_in?: number;
  sse_token?: string;
  sse_expires_in?: number;
  session_secret?: string;
}

export function useOccupySeat() {
  const api = useApi();
  const auth = useAuth();
  const toast = useToast();

  const open = ref(false);
  const loading = ref(false);
  const saving = ref(false);
  const seats = ref<OccupiableSeat[]>([]);
  const positionId = ref('');
  const personalUsername = ref('');
  const password = ref('');

  const currentUsername = computed(() => {
    const user = auth.getUser();
    return String(user?.username || user?.name || '').trim();
  });

  async function loadSeats(): Promise<void> {
    loading.value = true;
    try {
      const result = await api.get<SeatListResponse>('/api/v2/seats');
      if (!result.ok) {
        toast.showToast('error', '席位列表加载失败');
        seats.value = [];
        return;
      }
      seats.value = Array.isArray(result.data?.items) ? result.data.items : [];
      const me = String(auth.getUser()?.sub || auth.getUser()?.id || '').trim();
      const occupied = seats.value.find((seat) => seat.current_occupant_user_id === me);
      if (occupied) {
        positionId.value = occupied.id;
      } else if (!positionId.value && seats.value[0]) {
        positionId.value = seats.value[0].id;
      }
    } finally {
      loading.value = false;
    }
  }

  async function openModal(): Promise<void> {
    personalUsername.value = currentUsername.value;
    password.value = '';
    open.value = true;
    await loadSeats();
  }

  function closeModal(): void {
    open.value = false;
    password.value = '';
  }

  async function occupy(): Promise<boolean> {
    if (!positionId.value.trim()) {
      toast.showToast('warning', '请选择岗位席位');
      return false;
    }
    if (!personalUsername.value.trim() || !password.value) {
      toast.showToast('warning', '请填写个人用户名和密码');
      return false;
    }
    saving.value = true;
    try {
      const result = await api.post<TokenResponse>(
        `/api/v2/seats/${encodeURIComponent(positionId.value.trim())}/occupy`,
        {
          personal_username: personalUsername.value.trim(),
          proof: { kind: 'password', password: password.value },
        },
      );
      if (!result.ok) {
        toast.showToast('error', '占席失败：用户名或密码不正确，或该席不可占用');
        return false;
      }
      if (result.data?.access_token) {
        auth.saveToken({
          access_token: result.data.access_token,
          refresh_token: result.data.refresh_token,
          expires_in: result.data.expires_in,
          sse_token: result.data.sse_token,
          sse_expires_in: result.data.sse_expires_in,
          session_secret: result.data.session_secret,
        });
      }
      toast.showToast('success', '已换人占席');
      closeModal();
      return true;
    } finally {
      saving.value = false;
    }
  }

  return {
    open,
    loading,
    saving,
    seats,
    positionId,
    personalUsername,
    password,
    openModal,
    closeModal,
    occupy,
  };
}

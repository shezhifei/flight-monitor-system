use async_trait::async_trait;

#[async_trait]
pub trait FlightCacheBackend: Send + Sync {
    async fn invalidate_single_flight_cache(&self, flight_id: &str);
    async fn refresh_single_flight_cache(&self, flight_id: &str, payload: &str);
    async fn invalidate_flights_cache(&self);
}

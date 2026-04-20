#[derive(Debug, Clone, PartialEq)]
pub struct OrderForm {
    pub symbol: String,
    pub side: String,
    pub qty: f64,
    pub limit_price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AmendForm {
    pub order_id: String,
    pub qty: f64,
    pub limit_price: Option<f64>,
}

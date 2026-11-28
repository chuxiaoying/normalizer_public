class PriceService {
    int quote(int units) { return units * 2; }
}
class QuoteController {
    private PriceService price;
    void setPrice(PriceService price) { this.price = price; }
    public int quote(int units) { PriceService linked = this.price; return linked.quote(units); }
}

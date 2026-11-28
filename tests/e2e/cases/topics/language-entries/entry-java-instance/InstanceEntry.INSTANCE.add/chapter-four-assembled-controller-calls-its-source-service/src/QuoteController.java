class PriceService {
    int quote(int units) { return units * 2; }
}
class QuoteController {
    private PriceService price;
    void setPrice(PriceService price) { this.price = price; }
    public int quote(int units) { return price.quote(units); }
}

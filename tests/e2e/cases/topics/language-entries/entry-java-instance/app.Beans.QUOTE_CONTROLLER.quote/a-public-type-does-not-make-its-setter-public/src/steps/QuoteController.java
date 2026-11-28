package steps;

public class QuoteController {
    private QuoteService price;

    private void setPrice(QuoteService price) {
        this.price = price;
    }

    public int quote(int units) {
        return price.calculate(units);
    }
}

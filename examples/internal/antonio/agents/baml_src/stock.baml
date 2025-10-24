// Fetch
class StockApiData {
    date string
    prices map<string, string>
}

// Extract from user prompt
class Stock {
    company_name string
    symbol string
}

// Provide tool call result to LLM
class StockPriceMessage {
    user_message string
    stock Stock
    price string @description(#"In USD"#)
}

// Execute tool in Baml
function get_stock_price(symbol: string) -> string {
    let url = "https://mastra-stock-data.vercel.app/api/stock-data?symbol=" + symbol;
    let data = baml.fetch_as<StockApiData>(url);
    let price = data.prices["4. close"];

    price
}

// Extract symbol from user prompt.
function ExtractStockData(message: string) -> Stock {
    client CustomGPT5Mini
    prompt #"
        You are a helpful assistant that provides current stock prices.
        When asked about a stock, extract the company and  stock symbol from it
        (e.g Apple -> AAPL)

        {{ ctx.output_format }}

        User message:

        {{ message }}
    "#
}

function GenerateStockResponse(message: StockPriceMessage) -> string {
    client CustomGPT5Mini
    prompt #"
        Generate a response for the user who asked about the price of a stock.
        Don't ask any other questions, simply answer the user's question.

        {{ message }}
    "#
}

function StockAgent(message: string) -> string {
    let stock = ExtractStockData(message);
    let price = get_stock_price(stock.symbol);

    GenerateStockResponse(StockPriceMessage {
        user_message: message,
        stock: stock,
        price: price
    })
}
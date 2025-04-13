#[cfg(test)]
mod tests {
    use super::*;
    use crate::itch::parser::ITCHParser;
    use crate::itch::book::OrderBookReconstructor;
    use crate::itch::types::*;
    use std::fs::File;
    // use std::io::Read;
    use std::path::PathBuf;

    #[test]
    fn test_parse_system_event() {
        // Test simple ITCH message parsing
        let mut parser = ITCHParser::new();
        
        // Create a simple System Event message
        let message_type = b'S'; // System Event
        let timestamp = 1234567890u64.to_be_bytes();
        let event_code = b'O'; // Start of Messages
        
        let mut message_data = vec![message_type];
        message_data.extend_from_slice(&timestamp);
        message_data.push(event_code);
        
        // Parse the message
        let result = parser.parse_message(&message_data);
        assert!(result.is_ok(), "Failed to parse system event: {:?}", result.err());
        
        let message = result.unwrap();
        assert_eq!(message.message_type, MessageType::SystemEvent);
        assert_eq!(message.timestamp, 1234567890);
        
        if let MessagePayload::SystemEvent(event) = &message.payload {
            assert_eq!(event.event_code, SystemEventCode::StartOfMessages);
        } else {
            panic!("Wrong message payload type");
        }
    }
    
    #[test]
    fn test_add_order_parsing() {
        // Test parsing an Add Order message
        let mut parser = ITCHParser::new();
        
        // Create an Add Order message
        let message_type = b'A'; // Add Order
        let timestamp = 9876543210u64.to_be_bytes();
        let order_ref = 12345u64.to_be_bytes();
        let buy_sell = b'B'; // Buy
        let shares = 100u32.to_be_bytes();
        let stock = b"AAPL    "; // 8 characters, space-padded
        let price = 17045u32.to_be_bytes(); // 170.45 in 4-digit fixed point
        
        let mut message_data = vec![message_type];
        message_data.extend_from_slice(&timestamp);
        message_data.extend_from_slice(&order_ref);
        message_data.push(buy_sell);
        message_data.extend_from_slice(&shares);
        message_data.extend_from_slice(stock);
        message_data.extend_from_slice(&price);
        
        // Parse the message
        let result = parser.parse_message(&message_data);
        assert!(result.is_ok(), "Failed to parse add order: {:?}", result.err());
        
        let message = result.unwrap();
        assert_eq!(message.message_type, MessageType::AddOrder);
        assert_eq!(message.timestamp, 9876543210);
        assert_eq!(message.stock.as_ref().unwrap(), "AAPL");
        
        if let MessagePayload::AddOrder(order) = &message.payload {
            assert_eq!(order.order_reference_number, 12345);
            assert_eq!(order.buy_sell_indicator, BuySellIndicator::Buy);
            assert_eq!(order.shares, 100);
            assert_eq!(order.stock, "AAPL");
            assert_eq!(order.price, 17045);
            assert_eq!(price_to_float(order.price), 170.45);
        } else {
            panic!("Wrong message payload type");
        }
    }
    
    #[test]
    fn test_order_book_reconstruction() {
        // Test order book reconstruction
        let mut parser = ITCHParser::new();
        let mut book = OrderBookReconstructor::new();
        
        // Add some orders
        let orders = [
            // Buy orders for AAPL
            create_add_order(1001, 'B', 100, "AAPL", 15000), // $1.50
            create_add_order(1002, 'B', 200, "AAPL", 14900), // $1.49
            create_add_order(1003, 'B', 300, "AAPL", 15100), // $1.51 (best bid)
            
            // Sell orders for AAPL
            create_add_order(2001, 'S', 150, "AAPL", 15200), // $1.52 (best ask)
            create_add_order(2002, 'S', 250, "AAPL", 15300), // $1.53
            create_add_order(2003, 'S', 350, "AAPL", 15250), // $1.525
            
            // Orders for MSFT
            create_add_order(3001, 'B', 500, "MSFT", 30000), // $3.00
            create_add_order(3002, 'S', 600, "MSFT", 30100), // $3.01
        ];
        
        // Process all the orders
        for msg_data in orders.iter() {
            let message = parser.parse_message(msg_data).unwrap();
            book.process_message(&message).unwrap();
        }
        
        // Check the order book for AAPL
        let aapl_book = book.get_order_book("AAPL").expect("Should have AAPL order book");
        
        // Verify bids (should be ordered highest to lowest)
        assert_eq!(aapl_book.bids.len(), 3, "Should have 3 bid levels");
        assert_eq!(aapl_book.bids[0].price, 1.51, "Best bid should be $1.51");
        assert_eq!(aapl_book.bids[0].size, 300, "Best bid size should be 300");
        assert_eq!(aapl_book.bids[1].price, 1.50, "Second bid should be $1.50");
        assert_eq!(aapl_book.bids[2].price, 1.49, "Third bid should be $1.49");
        
        // Verify asks (should be ordered lowest to highest)
        assert_eq!(aapl_book.asks.len(), 3, "Should have 3 ask levels");
        assert_eq!(aapl_book.asks[0].price, 1.52, "Best ask should be $1.52");
        assert_eq!(aapl_book.asks[0].size, 150, "Best ask size should be 150");
        assert_eq!(aapl_book.asks[1].price, 1.525, "Second ask should be $1.525");
        assert_eq!(aapl_book.asks[2].price, 1.53, "Third ask should be $1.53");
        
        // Check the MSFT book
        let msft_book = book.get_order_book("MSFT").expect("Should have MSFT order book");
        assert_eq!(msft_book.bids.len(), 1, "Should have 1 bid level for MSFT");
        assert_eq!(msft_book.asks.len(), 1, "Should have 1 ask level for MSFT");
        
        // Test order execution
        let exec_msg = create_order_executed(1001, 50);
        let message = parser.parse_message(&exec_msg).unwrap();
        book.process_message(&message).unwrap();
        
        // Verify the order was partially executed
        let aapl_book = book.get_order_book("AAPL").expect("Should have AAPL order book");
        assert_eq!(aapl_book.bids[1].size, 50, "Bid size should be reduced to 50");
        
        // Test order deletion
        let delete_msg = create_order_delete(1002); // Delete the $1.49 bid
        let message = parser.parse_message(&delete_msg).unwrap();
        book.process_message(&message).unwrap();
        
        // Verify the order was deleted
        let aapl_book = book.get_order_book("AAPL").expect("Should have AAPL order book");
        assert_eq!(aapl_book.bids.len(), 2, "Should have 2 bid levels after deletion");
        assert!(aapl_book.bids.iter().all(|bid| bid.price != 1.49), "The $1.49 level should be gone");
    }
    
    // Helper function to create an Add Order message
    fn create_add_order(order_ref: u64, side: char, shares: u32, stock: &str, price: u32) -> Vec<u8> {
        let message_type = b'A'; // Add Order
        let timestamp = 9876543210u64.to_be_bytes();
        let order_ref = order_ref.to_be_bytes();
        let buy_sell = if side == 'B' { b'B' } else { b'S' };
        let shares = shares.to_be_bytes();
        
        // Create space-padded stock symbol (8 chars)
        let mut stock_bytes = [b' '; 8];
        for (i, byte) in stock.as_bytes().iter().enumerate().take(8) {
            stock_bytes[i] = *byte;
        }
        
        let price = price.to_be_bytes();
        
        let mut message_data = vec![message_type];
        message_data.extend_from_slice(&timestamp);
        message_data.extend_from_slice(&order_ref);
        message_data.push(buy_sell);
        message_data.extend_from_slice(&shares);
        message_data.extend_from_slice(&stock_bytes);
        message_data.extend_from_slice(&price);
        
        message_data
    }
    
    // Helper function to create an Order Executed message
    fn create_order_executed(order_ref: u64, executed_shares: u32) -> Vec<u8> {
        let message_type = b'E'; // Order Executed
        let timestamp = 9876543220u64.to_be_bytes();
        let order_ref = order_ref.to_be_bytes();
        let executed_shares = executed_shares.to_be_bytes();
        let match_number = 9876u64.to_be_bytes();
        
        let mut message_data = vec![message_type];
        message_data.extend_from_slice(&timestamp);
        message_data.extend_from_slice(&order_ref);
        message_data.extend_from_slice(&executed_shares);
        message_data.extend_from_slice(&match_number);
        
        message_data
    }
    
    // Helper function to create an Order Delete message
    fn create_order_delete(order_ref: u64) -> Vec<u8> {
        let message_type = b'D'; // Order Delete
        let timestamp = 9876543230u64.to_be_bytes();
        let order_ref = order_ref.to_be_bytes();
        
        let mut message_data = vec![message_type];
        message_data.extend_from_slice(&timestamp);
        message_data.extend_from_slice(&order_ref);
        
        message_data
    }
}

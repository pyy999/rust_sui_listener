use std::io::Read;
use curl::easy::Easy;

const HTTP_RETRY_LIMIT: i32 = 5;
const CHECKPOINTS_TO_READ: i32 = 9;
const CHECKPOINTS_PAGE_SIZE: i32 = 1;

fn main() {
    let transaction_manager: TransactionManager = TransactionManager{};
    let manager: QueryManager = QueryManager{};
    manager.start(&transaction_manager);
}

struct Transaction {
    address: String,
    amount: i64,
}

struct TransactionManager {

}

impl TransactionManager {
    /*
        todo: filter by tracked addresses
    */
    fn handle_transactions(&self, transactions: &Vec<Transaction>) {
        for transaction in transactions {
            println!("transaction: address={}, amount={}", transaction.address, transaction.amount);
        }
    }
}

struct QueryManager {
}

impl QueryManager {
    /*
        fetches the latest checkpoint, and uses the returned cursor to poll for the next checkpoints
     */
    fn start(&self, transaction_manager: &TransactionManager) {
        let mut latest_checkpoint_retry_counter = 0;
        let latest_checkpoint_cursor: Option<(String, String)> = loop {
            latest_checkpoint_retry_counter += 1;
            if let Some(result) = self.query_latest_checkpoint(){
                break Some(result);
            }

            if latest_checkpoint_retry_counter == HTTP_RETRY_LIMIT {
                break None;
            }
        };

        let (mut cursor, _) = latest_checkpoint_cursor.unwrap();
        let mut counter = 0;
        {
            //println!("Last checkpoint: {}", cursor);

            while counter < CHECKPOINTS_TO_READ {
                cursor = self.query_checkpoints(cursor.clone(), transaction_manager);
                counter += 1;
            }
        }
    }

    /*
        curl requests for checkpoint specified by cursor
        "publishes" transaction info 
        returns end cursor

        if any rest call isn't 200 OK, retries up to HTTP_RETRY_LIMIT

        NOT IMPLEMENTED: parsing and handling "hasNextPage". Program will error out upon reaching the last checkpoint
     */
    fn query_checkpoints(&self, cursor: String, transaction_manager: &TransactionManager) -> String {
        //println!("Tailing Cursor: {}", cursor);

        let mut checkpoint_retry_counter = 0;
        loop {
            checkpoint_retry_counter += 1;
            if let Some(result) = self.query_single_checkpoint(&cursor) {
                let (_, transactions) = result;
                transaction_manager.handle_transactions(&transactions);
                break;
            }

            if checkpoint_retry_counter == HTTP_RETRY_LIMIT {
                break;
            }
        };

        // let (cursor, digests) = checkpoints.unwrap();
        // for n in 0..digests.len() {
        //     let mut transaction_retry_counter = 0;
        //     loop {
        //         transaction_retry_counter += 1;
        //         if let Some(result) = self.query_digest(digests.get(n as usize).unwrap()) {
        //             transaction_manager.handle_transaction(&result);
        //             break;
        //         }

        //         if transaction_retry_counter == HTTP_RETRY_LIMIT {
        //             break;
        //         }
        //     };
        // }
        cursor
    }
    
    /*
        rest POST
     */
    fn do_query(&self, mut input: &[u8]) -> Option<String> {
        let mut headers = curl::easy::List::new();
        headers.append("x-sui-rpc-show-usage: true").unwrap();
        headers.append("Content-Type: application/json").unwrap();

        let mut easy = Easy::new();
        //easy.url("https://sui-testnet.mystenlabs.com/graphql").unwrap();
        easy.url("https://sui-mainnet.mystenlabs.com/graphql").unwrap();
        easy.post(true).unwrap();
        easy.post_field_size(input.len() as u64).unwrap();
        easy.http_headers(headers).unwrap();
    
        let mut html_data: String = String::new();
        { 
            let mut transfer = easy.transfer();
            transfer.read_function(|buf| {
                Ok(input.read(buf).unwrap_or(0))
            }).unwrap();

            transfer.write_function(|data| {
                html_data = String::from_utf8(Vec::from(data)).unwrap();
                // println!("{}", html_data);
                Ok(data.len())
            }).unwrap();
            transfer.perform().unwrap();
        }

        // println!("response: {}", easy.response_code().unwrap());
        if easy.response_code().unwrap() != 200 {
            return None
        }

        Some(html_data)
    }

    /*
        This queries the *10th* latest checkpoint- that way I don't have to wait to poll the latest checkpoint

        returns (cursor, digest)
     */
    fn query_latest_checkpoint(&self) -> Option<(String, String)> {
        let data: &str = "{\"query\": \"query ($before: String) { checkpoints(last: 10, before: $before) { pageInfo { startCursor } nodes { digest timestamp } }}\"}";
        //println!("query: {}", data);
        let query = data.as_bytes();

        let html_data = self.do_query(query);
        //println!("latest checkpoint query: {}", html_data.as_ref().unwrap());
        self.parse_start_checkpoint(&html_data.unwrap())
    }

    /*
        returns (next_cursor, vector of digests as strings)
     */
    fn query_single_checkpoint(&self, cursor: &String) -> Option<(String, Vec<Transaction>)> {
        let mut data = "{\"query\": \"query ($after: String) { checkpoints(first: ".to_owned();
        data.push_str(CHECKPOINTS_PAGE_SIZE.to_string().as_str());
        let data2 = ", after: $after) { pageInfo { hasNextPage endCursor } nodes { timestamp transactionBlocks { edges { node { effects { balanceChanges { nodes { owner { address } amount}}} }}}} }}\", ";
        data.push_str(data2);
        let vars = "\"variables\": { \"after\": \"";
        data.push_str(vars);
        data.push_str(cursor.as_str());
        data.push_str("\"}}");
        //println!("query: {}", data);
        let query = data.as_bytes();

        let html_data = self.do_query(query);
        //println!("checkpoint query: {}", html_data.as_ref().unwrap());
        // self.parse_checkpoint_query_for_digest(&html_data.unwrap())
        self.parse_checkpoint_query_for_transaction_info(&html_data.unwrap())
    }

    // fn query_digest(&self, digest: &String) -> Option<Transaction> {
    //     let mut data = "{\"query\": \"query { transactionBlock(digest: \\\"".to_owned();
    //     data.push_str(&digest.as_str());
    //     let data2 = "\\\") { gasInput { gasSponsor { address } gasPrice gasBudget } effects { status timestamp checkpoint { sequenceNumber } epoch { epochId referenceGasPrice }}}}\"}";
    //     data.push_str(data2);
    //     //println!("digest: {}", data);
    //     let query = data.as_bytes();

    //     let html_data = self.do_query(query);
    //     //println!("checkpoint query: {}", html_data.as_ref().unwrap());
    //     self.parse_transaction(&digest, &html_data.unwrap())
    // }

    // fn parse_digest(&self, data: &String) -> Option<String> {
    //     let digest_position_start = data.find("\"digest\":\"").map(|i| i + 10);
    //     let digest_position_end = data[digest_position_start.unwrap()..].find("\"").map(|i| i + digest_position_start.unwrap());
    //     let digest = &data[digest_position_start.unwrap()..digest_position_end.unwrap()];
    //     //println!("digest: {}", digest);
    //     Some(digest.to_string())
    // }

    fn parse_start_checkpoint(&self, html_data: &String) -> Option<(String, String)> {
        let start_cursor_position_start = html_data.find("\"startCursor\":\"").map(|i| i + 15);
        let start_cursor_position_end = html_data[start_cursor_position_start.unwrap()..].find("\"").map(|i| i + start_cursor_position_start.unwrap());
        let start_cursor = &html_data[start_cursor_position_start.unwrap()..start_cursor_position_end.unwrap()];
        //println!("start_cursor: {}", start_cursor);

        let mut digest_position_start = html_data.find("\"");
        let temp = html_data[digest_position_start.unwrap()..].find("\"digest\":\"");
        //println!("digest start, {}, temp {}, string: {}", digest_position_start.unwrap(), temp.unwrap(), html_data[digest_position_start.unwrap()..].to_string());
        digest_position_start = temp.map(|i| i + 10 + digest_position_start.unwrap());
        let digest_position_end = html_data[digest_position_start.unwrap()+1..].find("\"").map(|i| i + digest_position_start.unwrap()+1);
        let digest = &html_data[digest_position_start.unwrap()..digest_position_end.unwrap()];
       
        Some((start_cursor.to_string(), digest.to_string()))
    }

    // fn parse_checkpoint_query_for_digest(&self, html_data: &String) -> Option<(String, Vec<String>)> {
    //     let mut digests: Vec<String> = Vec::new();
    //     println!("checkpoint data: {}", html_data);

    //     let end_cursor_position_start = html_data.find("\"endCursor\":\"").map(|i| i + 13);
    //     let end_cursor_position_end = html_data[end_cursor_position_start.unwrap()..].find("\"").map(|i| i + end_cursor_position_start.unwrap());
    //     let end_cursor = &html_data[end_cursor_position_start.unwrap()..end_cursor_position_end.unwrap()];

    //     let mut digest_position_start = html_data.find("\"");
    //     loop { 
    //         let temp = html_data[digest_position_start.unwrap()..].find("\"digest\":\"");

    //         if temp == None { // parse until no more digests in transaction block edges list
    //             break
    //         }
    //         //println!("digest start, {}, temp {}, string: {}", digest_position_start.unwrap(), temp.unwrap(), html_data[digest_position_start.unwrap()..].to_string());
    //         digest_position_start = temp.map(|i| i + 10 + digest_position_start.unwrap());
    //         let digest_position_end = html_data[digest_position_start.unwrap()+1..].find("\"").map(|i| i + digest_position_start.unwrap()+1);
    //         let digest = &html_data[digest_position_start.unwrap()..digest_position_end.unwrap()];
    //         //println!("digest: {}, {}->{}", digest, digest_position_start.unwrap(), digest_position_end.unwrap());
    //         digests.push(digest.to_string());
    //     }
        
    //     Some((end_cursor.to_string(), digests))
    // }

    fn parse_checkpoint_query_for_transaction_info(&self, html_data: &String) -> Option<(String, Vec<Transaction>)> {
        let mut transactions: Vec<Transaction> = Vec::new();
       // println!("checkpoint data: {}", html_data);

        let end_cursor_position_start = html_data.find("\"endCursor\":\"").map(|i| i + 13);
        let end_cursor_position_end = html_data[end_cursor_position_start.unwrap()..].find("\"").map(|i| i + end_cursor_position_start.unwrap());
        let end_cursor = &html_data[end_cursor_position_start.unwrap()..end_cursor_position_end.unwrap()];

        let mut address_start = html_data.find("\"");
        loop { 
            let temp = html_data[address_start.unwrap()..].find("\"address\":\"");

            if temp == None { // parse until no more addresses in transaction blocks
                break
            }

            address_start = temp.map(|i| i + 11 + address_start.unwrap());
            let address_end = html_data[address_start.unwrap()+1..].find("\"").map(|i| i + address_start.unwrap()+1);
            let address = &html_data[address_start.unwrap()..address_end.unwrap()];

            let amount_start = html_data[address_start.unwrap()..].find("\"amount\":\"").map(|i| i + 10 + address_start.unwrap());
            let amount_end = html_data[amount_start.unwrap()..].find("\"").map(|i| i + amount_start.unwrap());
            let amount = &html_data[amount_start.unwrap()..amount_end.unwrap()];

            //println!("address: {}, amount: {}, {}->{}", address, amount, address_start.unwrap(), address_end.unwrap());
            transactions.push(Transaction{address: address.to_string(), amount: amount.parse::<i64>().unwrap()});
        }
        
        Some((end_cursor.to_string(), transactions))
    }

    // fn parse_transaction(&self, digest: &String, html_data: &String) -> Option<Transaction> {
    //     //println!("transaction: digest={} response={}", digest, html_data);

    //     let status_position_start = html_data.find("\"status\":\"").map(|i| i + 10);
    //     let status_position_end = html_data[status_position_start.unwrap()..].find("\"").map(|i| i + status_position_start.unwrap());
    //     let status = &html_data[status_position_start.unwrap()..status_position_end.unwrap()];
        
    //     if status != "SUCCESS" {
    //         return None;
    //     }

    //     let address_position_start = html_data.find("\"address\":\"").map(|i| i + 11);
    //     let address_position_end = html_data[address_position_start.unwrap()..].find("\"").map(|i| i + address_position_start.unwrap());
    //     let address = &html_data[address_position_start.unwrap()..address_position_end.unwrap()];

    //     let gas_price_position_start = html_data.find("\"gasPrice\":\"").map(|i| i + 12);
    //     let gas_price_position_end = html_data[gas_price_position_start.unwrap()..].find("\"").map(|i| i + gas_price_position_start.unwrap());
    //     let gas_price = &html_data[gas_price_position_start.unwrap()..gas_price_position_end.unwrap()];

    //     let gas_budget_position_start = html_data.find("\"gasBudget\":\"").map(|i| i + 13);
    //     let gas_budget_position_end = html_data[gas_budget_position_start.unwrap()..].find("\"").map(|i| i + gas_budget_position_start.unwrap());
    //     let gas_budget = &html_data[gas_budget_position_start.unwrap()..gas_budget_position_end.unwrap()];

    //     Some(Transaction{address: address.to_string(), gasPrice: gas_price.parse::<i32>().unwrap()})
    // }
}

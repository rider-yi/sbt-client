extern crate serde_json;
extern crate regex;

use regex::Regex;
use std::io::Read;
use sbtclient::{Message, SbtClientError};
use sbtclient::Message::Response;
use sbtclient::util::{error, detailed_error};

pub struct HeaderParser {
    content_length_header_regex: Regex
}

impl HeaderParser {

    pub fn new() -> HeaderParser {
        HeaderParser {
            content_length_header_regex: Regex::new(r"Content-Length: (\d+)").unwrap()
        }
    }

    pub fn extract_content_length(&self, raw_headers: String) -> Result<usize, SbtClientError> {
        let captures = self.content_length_header_regex.captures(&raw_headers)
            .ok_or(error("Failed to extract content length from headers"))?;
        let capture = captures.get(1)
            .ok_or(error("Failed to extract content length from headers"))?;
        capture.as_str().parse::<usize>()
            .map_err(|e| detailed_error("Failed to extract content length from headers", e))
    }

}

/*
 * Receives, deserializes and renders the next message from the server.
 * Returns true if it was the final message in response to our command, meaning we can stop looping.
 */
pub fn receive_next_message<S: Read>(stream: &mut S, header_parser: &HeaderParser, handle_message: fn(Message) -> ()) -> Result<bool, SbtClientError> {
    let headers = read_headers(stream)?;
    let content_length = header_parser.extract_content_length(headers)?;
    let mut buf: Vec<u8> = Vec::with_capacity(content_length);
    buf.resize(content_length, 0);
    stream.read_exact(&mut buf)
        .map_err(|e| detailed_error("Failed to read bytes from Unix socket", e))?;
    let raw_json = String::from_utf8(buf.to_vec())
        .map_err(|e| detailed_error("Failed to decode message as UTF-8 string", e))?;
    let message: Message = serde_json::from_str(&raw_json)
        .map_err(|e| detailed_error(&format!("Failed to deserialize message from JSON '{}'", raw_json), e))?;
    let received_result = match message {
        Response { id, .. } if id == 1 => true,
        _ => false
    };
    handle_message(message);
    Ok(received_result)
}

fn read_headers<S: Read>(stream: &mut S) -> Result<String, SbtClientError> {
    let mut headers = Vec::with_capacity(1024);
    let mut one_byte = [0];
    while !ends_with_double_newline(&headers) {
        try! (
            stream.read_exact(&mut one_byte)
                .map(|_| headers.push(one_byte[0]))
                .map_err(|e| detailed_error("Failed to read next byte of headers", e))
        )
    }
    String::from_utf8(headers)
        .map_err(|e| detailed_error("Failed to read headers as a UTF-8 string", e))
}

fn ends_with_double_newline(vec: &Vec<u8>) -> bool {
    vec.ends_with(&[13, 10, 13, 10])
}

#[cfg(test)]
mod tests {
    use super::*;
    use sbtclient::*;
    use sbtclient::Message::*;

    #[test]
    fn receive_successful_result() {
        let mut lsp_message = "Content-Type: application/vscode-jsonrpc; charset=utf-8\r
Content-Length: 126\r
\r
{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"status\":\"Done\",\"channelName\":\"network-1\",\"execId\":1,\"commandQueue\":[\"shell\"],\"exitCode\":0}}".as_bytes();

        let assertion = |msg: Message| {
            let expected = Response {
                id: 1,
                result: CommandResult {
                    status: "Done".to_string(),
                    exit_code: 0
                }
            };
            assert_eq!(expected, msg);
        };

        let received_final_message = receive_next_message(
            &mut lsp_message,
            &HeaderParser::new(),
            assertion).unwrap();

        assert_eq!(true, received_final_message);
    }

    #[test]
    fn receive_log_message() {
        let mut lsp_message = "Content-Type: application/vscode-jsonrpc; charset=utf-8\r
Content-Length: 89\r
\r
{\"jsonrpc\":\"2.0\",\"method\":\"window/logMessage\",\"params\":{\"type\":4,\"message\":\"Processing\"}}".as_bytes();

        let assertion = |msg: Message| {
            let expected = LogMessage {
                method: "window/logMessage".to_string(),
                params: LogMessageParams {
                    type_: 4,
                    message: "Processing".to_string(),
                }
            };
            assert_eq!(expected, msg);
        };

        let received_final_message = receive_next_message(
            &mut lsp_message,
            &HeaderParser::new(),
            assertion).unwrap();

        assert_eq!(false, received_final_message);
    }

}

pub mod errors;
pub mod kwik;
pub mod parser;
pub mod utils;

pub use errors::{KwikError, Result};
pub use kwik::{DirectLink, KwikClient};

#[cfg(test)]
mod test {
    use crate::utils::*;

    #[test]
    fn test_embed_unpack() {
        const PACKED: &str = r#""j q='1N://1M-H.1L.1K/1J/H/1I/1H/1G.1F';j g=u.t('g');j 3=D 1E(g,{'1D':{'1C':k},'1B':'16:9','G':1,'1A':5,'1z':{1y:1,1x:[0.1w,1,1.1,1.15,1.2,1.1v,1.5,2,4]},'1u':{'1t':'1s'},'1r':['a-1q','a','1p','1o-1n','1m','1l-1k','1j','G','1i','1h','1g','1f','F','1e'],'F':{'1d':k}});b(!C.1c()){g.1b=q}z{l B={1a:19,18:17*E*E,14:13,12:11,10:9,Z:k,Y:k};j f=D C(B);f.X(q);f.W(g);i.f=f}3.6(\"V\",8=>{i.U.T.S(\"R\")});m x(d,p,o){b(d.A){d.A(p,o,Q)}z b(d.y){d.y('6'+p,o)}}l 7=m(n){i.P.O(n,'*')};x(i,'n',m(e){l c=e.c;b(c==='a')3.a();b(c==='h')3.h();b(c==='w')3.w()});3.6('v',8=>{7('v')});3.6('a',8=>{7('a')});3.6('h',8=>{7('h')});3.6('N',8=>{7(3.s);u.t('.M-L').K=J(3.s.I(2))});3.6('r',8=>{7('r')});""#;
        const BASE: u32 = 62;
        const COUNT: u32 = 62;
        const SYMTAB: &str = r#"|||player|||on|sendMessage|event||play|if|data|element||hls|video|pause|window|const|true|var|function|message|eventHandler|eventName|source|ended|currentTime|querySelector|document|ready|stop|bindEvent|attachEvent|else|addEventListener|config|Hls|new|1000|fullscreen|volume|99|toFixed|String|innerHTML|timestamp|ss|timeupdate|postMessage|parent|false|landscape|lock|orientation|screen|enterfullscreen|attachMedia|loadSource|lowLatencyMode|enableWorker|nudgeMaxRetry|600|maxMaxBufferLength|300|maxBufferLength|||120|maxBufferSize|90|backBufferLength|src|isSupported|iosNative|capture|airplay|pip|settings|captions|mute|time|current|progress|forward|fast|rewind|large|controls|kwik|key|storage|25|75|options|selected|speed|seekTime|ratio|global|keyboard|Plyr|m3u8|uwu|919045153925ac63c1942f913a856ac0d4241a9ca31527522f6f6f77751d1055|02|stream|top|owocdn|vault|https"#;

        let symtab: Vec<&str> = SYMTAB.split('|').collect();
        let unpacked = unpack_de(PACKED, BASE, COUNT as usize, symtab);

        assert!(!unpacked.is_empty())
    }
}



extern crate nom;
use nom::{ digit, hex_digit, oct_digit, line_ending, not_line_ending, space, multispace, is_alphanumeric, is_alphabetic, IResult };

extern crate std;
use std::f64;
use std::str;
use std::str::FromStr;

use scope::{ Scope, ScopeRef };
use types::Type;


named!(sp, eat_separator!(&b" \t"[..]));

#[macro_export]
macro_rules! sp (
    ($i:expr, $($args:tt)*) => (
        {
            sep!($i, sp, $($args)*)
        }
    )
);

#[macro_export]
macro_rules! wscom {
    ($i:expr, $submac:ident!( $($args:tt)* )) => ({
        use parser::multispace_comment;
        //terminated!($i, $submac!($($args)*), multispace_comment)
        //preceded!($i, multispace_comment, $submac!($($args)*))
        //sep!($i, multispace_comment, $submac!($($args)*))
        delimited!($i, multispace_comment, $submac!($($args)*), multispace_comment)
    });
    ($i:expr, $f:expr) => (
        wscom!($i, call!($f));
    );
}

#[macro_export]
macro_rules! wscomp {
    ($i:expr, $submac:ident!( $($args:tt)* )) => ({
        use parser::multispace_comment;
        //terminated!($i, $submac!($($args)*), multispace_comment)
        preceded!($i, multispace_comment, $submac!($($args)*))
        //sep!($i, multispace_comment, $submac!($($args)*))
        //delimited!($i, multispace_comment, $submac!($($args)*), multispace_comment)
    });
    ($i:expr, $f:expr) => (
        wscom!($i, call!($f));
    );
}

#[macro_export]
macro_rules! tag_word (
    ($i:expr, $f:expr) => (
        {
            do_parse!($i,
                s: tag!($f) >>
                not!(peek!(alphanumeric_underscore)) >>
                ( s )
            )
        }
    )
);

#[macro_export]
macro_rules! map_str (
    ($i:expr, $($args:tt)*) => (
        {
            map!($i, $($args)*, |s| String::from(str::from_utf8(s).unwrap()))
        }
    )
);


#[derive(Clone, Debug, PartialEq)]
pub enum AST {
    Noop,
    //Comment(String),

    Nil,
    Underscore,
    Boolean(bool),
    Integer(isize),
    Real(f64),
    String(String),
    List(Vec<AST>),

    Identifier(String),
    Index(Box<AST>, Box<AST>),
    Accessor(Box<AST>, Box<AST>),
    Invoke(String, Vec<AST>),
    //Prefix(String, Box<AST>),
    //Infix(String, Box<AST>, Box<AST>),
    Block(Vec<AST>),
    If(Box<AST>, Box<AST>, Box<AST>),
    Raise(Box<AST>),
    Try(Box<AST>, Vec<(AST, AST)>),
    Match(Box<AST>, Vec<(AST, AST)>),
    For(String, Box<AST>, Box<AST>, ScopeRef),
    Function(Vec<(String, Option<Type>, Option<AST>)>, Box<AST>, ScopeRef),
    Class(String, Vec<AST>, ScopeRef),

    Import(String),
    Definition((String, Option<Type>), Box<AST>),
    While(Box<AST>, Box<AST>),
    Type(String, Vec<(String, Option<Type>)>),
}


named!(pub parse<Vec<AST>>,
    complete!(do_parse!(
        e: many0!(statement) >>
        eof!() >>
        (e)
    ))
);

named!(statement<AST>,
    //separated_list!(ws!(tag!(",")), do_parse!(
    do_parse!(
        s: wscom!(alt!(
            import |
            definition |
            whileloop |
            typedef |
            expression
            //value!(AST::Noop, multispace_comment)
        )) >>
        //eat_separator!("\n") >>
        separator >>
        (s)
    )
);

named!(import<AST>,
dbg_dmp!(
    do_parse!(
        wscom!(tag_word!("import")) >>
        e: map_str!(recognize!(separated_list!(tag!("."), identifier))) >>
        (AST::Import(e))
    )
)
);

named!(definition<AST>,
    do_parse!(
        wscom!(tag_word!("let")) >>
        i: identifier_typed >>
        wscom!(tag!("=")) >>
        e: expression >>
        (AST::Definition(i, Box::new(e)))
    )
);

named!(whileloop<AST>,
    do_parse!(
        wscom!(tag_word!("while")) >>
        c: expression >>
        opt!(multispace_comment) >>
        e: expression >>
        (AST::While(Box::new(c), Box::new(e)))
    )
);

named!(typedef<AST>,
    do_parse!(
        wscom!(tag_word!("type")) >>
        i: identifier >>
        wscom!(tag!("=")) >>
        s: alt!(
            map!(identifier_typed, |i| vec!(i)) |
            delimited!(wscom!(tag!("{")), separated_list!(wscom!(tag!(",")), identifier_typed), wscom!(tag!("}")))
        ) >>
        (AST::Type(i, s))
    )
);



named!(expression<AST>,
    alt_complete!(
        noop |
        block |
        ifexpr |
        trywith |
        raise |
        matchcase |
        forloop |
        function |
        class |
        infix
    )
);

named!(noop<AST>,
    value!(AST::Noop, tag_word!("noop"))
);

named!(block<AST>,
    delimited!(
        wscom!(alt!(tag_word!("begin") | tag!("{"))),
        do_parse!(
            s: many0!(statement) >>
            (AST::Block(s))
        ),
        wscom!(alt!(tag_word!("end") | tag!("}")))
    )
);

named!(ifexpr<AST>,
    do_parse!(
        wscom!(tag_word!("if")) >>
        c: expression >>
        wscom!(tag_word!("then")) >>
        t: expression >>
        wscom!(tag_word!("else")) >>
        f: expression >>
        (AST::If(Box::new(c), Box::new(t), Box::new(f)))
    )
);

named!(trywith<AST>,
    do_parse!(
        wscom!(tag_word!("try")) >>
        c: expression >>
        wscom!(tag_word!("with")) >>
        l: caselist >>
        (AST::Try(Box::new(c), l))
    )
);

named!(raise<AST>,
    do_parse!(
        wscom!(tag_word!("raise")) >>
        e: expression >>
        (AST::Raise(Box::new(e)))
    )
);

named!(matchcase<AST>,
    do_parse!(
        wscom!(tag_word!("match")) >>
        c: expression >>
        wscom!(tag_word!("with")) >>
        l: caselist >>
        (AST::Match(Box::new(c), l))
    )
);

named!(caselist<Vec<(AST, AST)>>,
dbg_dmp!(
    //separated_list!(wscom!(tag!(",")), do_parse!(
    many1!(do_parse!(
        //wscom!(tag!("|")) >>
        c: alt_complete!(value!(AST::Underscore, tag!("_")) | literal) >>
        wscom!(tag!("->")) >>
        e: expression >>
        //wscom!(tag!(",")) >>
        (c, e)
    ))
)
);

named!(forloop<AST>,
    do_parse!(
        wscom!(tag_word!("for")) >>
        i: identifier >>
        wscom!(tag_word!("in")) >>
        l: expression >>
        opt!(multispace_comment) >>
        e: expression >>
        (AST::For(i, Box::new(l), Box::new(e), Scope::new_ref(None)))
    )
);

named!(function<AST>,
    do_parse!(
        wscom!(tag_word!("fn")) >>
        l: identifier_list_defaults >>
        wscom!(tag!("->")) >>
        e: expression >>
        (AST::Function(l, Box::new(e), Scope::new_ref(None)))
    )
);

named!(identifier_list<Vec<(String, Option<Type>)>>,
    separated_list!(tag!(","), identifier_typed)
);

named!(identifier_list_defaults<Vec<(String, Option<Type>, Option<AST>)>>,
    separated_list!(tag!(","),
        do_parse!(
            i: identifier_typed >>
            d: opt!(preceded!(tag!("="), expression)) >>
            ((i.0, i.1, d))
        )
    )
);

named!(class<AST>,
    do_parse!(
        wscom!(tag_word!("class")) >>
        i: identifier >>
        wscom!(tag!("{")) >>
        s: many0!(statement) >>
        wscom!(tag!("}")) >>
        (AST::Class(i, s, Scope::new_ref(None)))
    )
);


named!(infix_op<String>,
    map_str!(
        alt!(
            tag!("*") |
            tag!("/") |
            tag!("^") |
            tag!("%") |
            tag!("+") |
            tag!("-") |
            tag!("<<") |
            tag!(">>") |
            tag!("<") |
            tag!(">") |
            tag!("<=") |
            tag!(">=") |
            tag!("==") |
            tag!("!=") |
            tag!("&") |
            tag!("|") |
            tag_word!("and") |
            tag_word!("or")
            //tag!("..") |
        )
        //tag!(".")
    )
);

impl AST {
    fn precedence(op: &str) -> i32 {
        match op {
            "*" | "/" | "%"         => 5,
            "+" | "-"               => 6,
            "<<" | ">>"             => 7,
            "<" | ">" | "<=" | ">=" => 8,
            "==" | "!="             => 9,
            "&"                     => 10,
            "|"                     => 12,
            "and"                   => 13,
            "or"                    => 14,
            _                       => 20,
        }
    }

    /*
    fn fold_op_old(left: AST, operations: Vec<(String, AST)>) -> Self {
        operations.into_iter().fold(left, |acc, pair| {
            //AST::Infix(pair.0, Box::new(acc), Box::new(pair.1))
            AST::Invoke(pair.0, vec!(acc, pair.1))
        })
    }
    */

    fn fold_op(left: AST, operations: Vec<(String, AST)>) -> Self {
        let mut operands: Vec<AST> = vec!();
        let mut operators: Vec<(String, i32)> = vec!();
        operands.push(left);

        for (next_op, next_ast) in operations {
            let p = AST::precedence(next_op.as_str());

            while !operators.is_empty() && operators.last().unwrap().1 <= p {
                let op = operators.pop().unwrap().0;
                let r2 = operands.pop().unwrap();
                let r1 = operands.pop().unwrap();
                //operands.push(AST::Infix(op, Box::new(r1), Box::new(r2)));
                operands.push(AST::Invoke(op, vec!(r1, r2)));
            }

            operators.push((next_op, p));
            operands.push(next_ast);
        }

        while !operators.is_empty() {
            let op = operators.pop().unwrap().0;
            let r2 = operands.pop().unwrap();
            let r1 = operands.pop().unwrap();
            //operands.push(AST::Infix(op, Box::new(r1), Box::new(r2)));
            operands.push(AST::Invoke(op, vec!(r1, r2)));
        }

        assert_eq!(operands.len(), 1);
        operands.pop().unwrap()
    }
}

/*
#[macro_export]
macro_rules! infixer (
    ($i:expr, $op:expr, $($sub:tt)*) => (
        {
            do_parse!($i,
                left: $($sub)* >>
                operations: many0!(do_parse!(
                    op: call!($op) >>
                    right: $($sub)* >>
                    (op, right)
                )) >>
                (AST::fold_op(left, operations))
            )
        }
    )
);

named!(infix<AST>,
    infixer!(infix_op, alt!(atomic))
);
*/

named!(infix<AST>,
    do_parse!(
        left: atomic >>
        operations: many0!(tuple!(infix_op, atomic)) >>
        (AST::fold_op(left, operations))
    )
);

named!(atomic<AST>,
    wscom!(alt_complete!(
        prefix |
        index |
        accessor |
        subatomic
    ))
);

named!(prefix_op<String>,
    map_str!(alt!(
        tag_word!("not") |
        tag!("~")
    ))
);

named!(prefix<AST>,
    do_parse!(
        op: prefix_op >>
        a: atomic >>
        //(AST::Prefix(op, Box::new(a)))
        (AST::Invoke(op, vec!(a)))
    )
);

named!(index<AST>,
    do_parse!(
        base: subatomic >>
        tag!("[") >>
        ind: expression >>
        tag!("]") >>
        (AST::Index(Box::new(base), Box::new(ind)))
    )
);

named!(accessor<AST>,
    do_parse!(
        left: subatomic >>
        //operations: many0!(tuple!(map_str!(tag!(".")), subatomic)) >>
        //(AST::fold_op(left, operations))
        tag!(".") >>
        right: atomic >>
        (AST::Accessor(Box::new(left), Box::new(right)))
    )
);

named!(subatomic<AST>,
    alt_complete!(
        literal |
        invoke |
        map!(identifier, |s| AST::Identifier(s)) |
        delimited!(tag!("("), wscom!(expression), tag!(")"))
    )
);

named!(invoke<AST>,
    do_parse!(
        s: identifier >>
        opt!(space) >>
        tag!("(") >>
        l: expression_list >>
        tag!(")") >>
        (AST::Invoke(s, l))
    )
);

named!(expression_list<Vec<AST>>,
    separated_list!(tag!(","), expression)
);

named!(identifier<String>,
    map_str!(
        do_parse!(
            not!(reserved) >>
            s: recognize!(preceded!(
                take_while1!(is_alpha_underscore),
                take_while!(is_alphanumeric_underscore)
            )) >>
            (s)
        )
    )
);

named!(identifier_typed<(String, Option<Type>)>,
    wscom!(do_parse!(
        i: identifier >>
        t: opt!(preceded!(wscom!(tag!(":")), type_description)) >>
        (i, t)
    ))
);

pub fn parse_type(s: &str) -> Option<Type> {
    match type_description(s.as_bytes()) {
        IResult::Done(_, t) => Some(t),
        _ => panic!("Error Parsing Type: {:?}", s)
    }
}

named!(type_description<Type>,
    alt!(
        type_function |
        type_variable |
        type_concrete
    )
);

named!(type_concrete<Type>,
    map!(identifier, |s| Type::Concrete(s))
);

named!(type_variable<Type>,
    map!(preceded!(tag!("'"), identifier), |s| Type::Variable(s))
);

named!(type_function<Type>,
    wscom!(do_parse!(
        args: delimited!(tag!("("), separated_list!(wscom!(tag!(",")), type_description), tag!(")")) >>
        wscom!(tag!("->")) >>
        ret: type_description >>
        (Type::Function(args, Box::new(ret)))
    ))
);

named!(reserved,
    alt!(
        tag_word!("do") | tag_word!("end") | tag_word!("while")
    )
);



named!(literal<AST>,
    alt_complete!(
        nil |
        boolean |
        string |
        number |
        list
    )
);

named!(nil<AST>,
    value!(AST::Nil, tag_word!("nil"))
);

named!(boolean<AST>,
    alt!(
        value!(AST::Boolean(true), tag_word!("true")) |
        value!(AST::Boolean(false), tag_word!("false"))
    )
);

named!(string<AST>,
    map!(
        delimited!(
            tag!("\""),
            is_not!("\""),
            tag!("\"")
        ),
        |s| AST::String(String::from(str::from_utf8(s).unwrap()))
    )
);

named!(number<AST>,
    alt_complete!(
        value!(AST::Real(std::f64::NEG_INFINITY), tag_word!("-Inf")) |
        value!(AST::Real(std::f64::INFINITY), tag_word!("Inf")) |
        value!(AST::Real(std::f64::NAN), tag_word!("NaN")) |
        oct_number |
        hex_number |
        int_or_float_number
    )
);

named!(hex_number<AST>,
    map!(
        preceded!(tag!("0x"), hex_digit),
        |s| AST::Integer(isize::from_str_radix(str::from_utf8(s).unwrap(), 16).unwrap())
    )
);

named!(oct_number<AST>,
    map!(
        preceded!(tag!("0"), oct_digit),
        |s| AST::Integer(isize::from_str_radix(str::from_utf8(s).unwrap(), 8).unwrap())
    )
);

named!(int_or_float_number<AST>,
    map!(
        recognize!(
            tuple!(
               opt!(tag!("-")),
               digit,
               opt!(complete!(preceded!(tag!("."), digit)))
               //opt!(complete!(float_exponent))
            )
        ),
        AST::number_from_utf8
    )
);

impl AST {
    fn number_from_utf8(s : &[u8]) -> Self {
        let n = str::from_utf8(s).unwrap();
        if let Ok(i) = isize::from_str_radix(n, 10) {
            AST::Integer(i)
        }
        else {
            AST::Real(f64::from_str(n).unwrap())
        }
    }
}

named!(list<AST>,
    map!(
        delimited!(
            wscom!(tag!("[")),
            separated_list!(wscom!(tag!(",")), expression),
            wscom!(tag!("]"))
        ),
        |e| AST::List(e)
    )
);




named!(separator,
dbg_dmp!(
    recognize!(many0!(
        //alt!(take_while1!(is_ws) | comment)
        //alt!(take_while1!(is_ws) | delimited!(tag!("//"), not_line_ending, line_ending))
        //terminated!(sp!(alt!(line_comment | block_comment)), line_ending)
        delimited!(space_comment, alt!(line_ending | tag!(";")), multispace_comment)
    ))
)
);



named!(space_comment,
    recognize!(many0!(alt!(line_comment | block_comment | space)))
);

named!(multispace_comment,
map!(
    recognize!(many0!(alt!(line_comment | block_comment | multispace))),
    |s| { count_lines(s); s }
)
);

named!(line_comment,
    delimited!(tag!("//"), not_line_ending, peek!(line_ending))    //, |s| AST::Comment(String::from(str::from_utf8(s).unwrap())))
);

named!(block_comment,
    delimited!(tag!("/*"), is_not!("*/"), tag!("*/"))              //, |s| AST::Comment(String::from(str::from_utf8(s).unwrap())))
);



named!(alphanumeric_underscore,
    take_while1!(is_alphanumeric_underscore)
);

pub fn is_alpha_underscore(ch: u8) -> bool {
    ch == b'_' || is_alphabetic(ch)
}

pub fn is_alphanumeric_underscore(ch: u8) -> bool {
    ch == b'_' || is_alphanumeric(ch)
}


static mut lines: usize = 0;

pub fn count_lines(text: &[u8]) {
    for ch in text {
        if *ch == '\n' as u8 {
            //*lines.get_mut() += 1;
            unsafe { lines += 1; }
        }
    }
}
 
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        assert_eq!(
            parse("
                5 * 3 + 8 / 100
            ".as_bytes()),
            IResult::Done(&b""[..], vec!(
                AST::Invoke(String::from("+"), vec!(
                    AST::Invoke(String::from("*"), vec!(AST::Integer(5), AST::Integer(3))),
                    AST::Invoke(String::from("/"), vec!(AST::Integer(8), AST::Integer(100)))
                ))
            ))
        );
    }
}


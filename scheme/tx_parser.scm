;; This extracted scheme code relies on some additional macros
;; available at http://www.pps.univ-paris-diderot.fr/~letouzey/scheme
(load "macros_extr.scm")


(define add (lambdas (n m) (+ n m)))
  
(define eqb (lambdas (n m) (if (= n m) `(True) `(False))))
  
(define leb (lambdas (n m) (if (< m n) `(False) `(True))))
  
(define u64_LEN 8)

(define aDDRESS_LEN 20)

(define tOKEN_ADDRESS_LEN 32)

(define mAX_TX_LEN 510)

(define mAX_MEMO_LEN 465)

(define pure (lambdas (a st) `(Inl ,`(Pair ,st ,a))))

(define bind (lambdas (p f st)
  (match (p st)
     ((Inl p0) (match p0
                  ((Pair st~ a) (@ f a st~))))
     ((Inr e) `(Inr ,e)))))

(define fail (lambdas (e _) `(Inr ,e)))

(define read_u64_be (lambdas (h err st)
  (let ((buf (match st
                ((Mk_parser_state ps_buffer _) ps_buffer))))
    (let ((off (match st
                  ((Mk_parser_state _ ps_offset) ps_offset))))
      (match (@ leb (@ add off u64_LEN)
               (match h
                  ((Build_BufferOps buf_length _ _ _) (buf_length buf))))
         ((True)
           (match (match h
                     ((Build_BufferOps _ buf_read_u64 _ _)
                       (@ buf_read_u64 buf off)))
              ((Some v) `(Inl ,`(Pair ,`(Mk_parser_state ,buf
                ,(@ add off u64_LEN)) ,v)))
              ((None) `(Inr ,err))))
         ((False) `(Inr ,err)))))))

(define read_slice (lambdas (h n err st)
  (let ((buf (match st
                ((Mk_parser_state ps_buffer _) ps_buffer))))
    (let ((off (match st
                  ((Mk_parser_state _ ps_offset) ps_offset))))
      (match (@ leb (@ add off n)
               (match h
                  ((Build_BufferOps buf_length _ _ _) (buf_length buf))))
         ((True) `(Inl ,`(Pair ,`(Mk_parser_state ,buf ,(@ add off n))
           ,`(Mk_slice ,off ,n))))
         ((False) `(Inr ,err)))))))

(define read_varint (lambdas (h err st)
  (let ((buf (match st
                ((Mk_parser_state ps_buffer _) ps_buffer))))
    (let ((off (match st
                  ((Mk_parser_state _ ps_offset) ps_offset))))
      (match (match h
                ((Build_BufferOps _ _ buf_read_varint _)
                  (@ buf_read_varint buf off)))
         ((Some p)
           (match p
              ((Pair value consumed)
                (match (@ leb (@ add off consumed)
                         (match h
                            ((Build_BufferOps buf_length _ _ _)
                              (buf_length buf))))
                   ((True) `(Inl ,`(Pair ,`(Mk_parser_state ,buf
                     ,(@ add off consumed)) ,value)))
                   ((False) `(Inr ,err))))))
         ((None) `(Inr ,err)))))))

(define guard (lambdas (cond err)
  (match cond
     ((True) (pure `(Tt)))
     ((False) (fail err)))))

(define check_offset_eq_size (lambdas (h err st)
  (match (@ eqb (match st
                   ((Mk_parser_state _ ps_offset) ps_offset))
           (match h
              ((Build_BufferOps buf_length _ _ _)
                (buf_length
                  (match st
                     ((Mk_parser_state ps_buffer _) ps_buffer))))))
     ((True) `(Inl ,`(Pair ,st ,`(Tt))))
     ((False) `(Inr ,err)))))

(define check_ascii (lambdas (h s err st)
  (match (match h
            ((Build_BufferOps _ _ _ buf_is_ascii)
              (@ buf_is_ascii
                (match st
                   ((Mk_parser_state ps_buffer _) ps_buffer))
                (match s
                   ((Mk_slice sl_offset _) sl_offset))
                (match s
                   ((Mk_slice _ sl_length) sl_length)))))
     ((True) `(Inl ,`(Pair ,st ,`(Tt))))
     ((False) `(Inr ,err)))))

(define parse_nonce (lambda (h) (@ read_u64_be h `(NonceParsingError))))

(define parse_to (lambda (h) (@ read_slice h aDDRESS_LEN `(ToParsingError))))

(define parse_token_address (lambda (h)
  (@ read_slice h tOKEN_ADDRESS_LEN `(TokenAddressParsingError))))

(define parse_value (lambda (h) (@ read_u64_be h `(ValueParsingError))))

(define parse_memo_len (lambda (h)
  (@ bind (@ read_varint h `(MemoLengthError)) (lambda (len)
    (@ bind (@ guard (@ leb len mAX_MEMO_LEN) `(MemoLengthError)) (lambda (_)
      (pure len)))))))

(define parse_memo (lambdas (h len)
  (@ bind (@ read_slice h len `(MemoParsingError)) (lambda (s)
    (@ bind (@ check_ascii h s `(MemoEncodingError)) (lambda (_) (pure s)))))))

(define parse_transaction (lambdas (h is_token)
  (@ bind (parse_nonce h) (lambda (nonce)
    (@ bind (parse_to h) (lambda (to)
      (@ bind
        (match is_token
           ((True)
             (@ bind (parse_token_address h) (lambda (a) (pure `(Some ,a)))))
           ((False) (pure `(None))))
        (lambda (tok_addr)
        (@ bind (parse_value h) (lambda (value)
          (@ bind (parse_memo_len h) (lambda (memo_len)
            (@ bind (@ parse_memo h memo_len) (lambda (memo)
              (@ bind (@ check_offset_eq_size h `(WrongLengthError))
                (lambda (_)
                (pure `(Mk_transaction ,nonce ,to ,tok_addr ,value ,memo))))))))))))))))))

(define transaction_deserialize (lambdas (h buf is_token)
  (let ((init `(Mk_parser_state ,buf ,`(0))))
    (match (@ leb
             (match h
                ((Build_BufferOps buf_length _ _ _) (buf_length buf)))
             mAX_TX_LEN)
       ((True)
         (match (@ parse_transaction h is_token init)
            ((Inl p) (match p
                        ((Pair _ tx) `(Inl ,tx))))
            ((Inr e) `(Inr ,e))))
       ((False) `(Inr ,`(WrongLengthError)))))))


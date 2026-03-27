(define wrap_some (lambda (x) `(Some ,x)))

(define option_map (lambdas (f o)
  (match o
    ((None_) `(None_))
    ((Some x) `(Some ,(f x))))))

(define option_bind (lambdas (f o)
  (match o
    ((None_) `(None_))
    ((Some x) (f x)))))

(define option_default (lambdas (d o)
  (match o
    ((None_) d)
    ((Some x) x))))

(define option_is_some (lambda (o)
  (match o
    ((None_) `(False))
    ((Some _) `(True)))))

(define option_is_none (lambda (o)
  (match o
    ((None_) `(True))
    ((Some _) `(False)))))

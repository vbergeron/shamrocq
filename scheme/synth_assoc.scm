(define assoc_get (lambdas (ord key l)
  (match l
    ((Nil) `(None_))
    ((Cons p rest) (match p
      ((Pair k v) (match (@ eqb ord key k)
        ((True) `(Some ,v))
        ((False) (@ assoc_get ord key rest)))))))))

(define assoc_set (lambdas (ord key val l)
  (match l
    ((Nil) `(Cons ,`(Pair ,key ,val) ,`(Nil)))
    ((Cons p rest) (match p
      ((Pair k v) (match (@ eqb ord key k)
        ((True) `(Cons ,`(Pair ,key ,val) ,rest))
        ((False) `(Cons ,p ,(@ assoc_set ord key val rest))))))))))

(define assoc_remove (lambdas (ord key l)
  (match l
    ((Nil) `(Nil))
    ((Cons p rest) (match p
      ((Pair k _) (match (@ eqb ord key k)
        ((True) rest)
        ((False) `(Cons ,p ,(@ assoc_remove ord key rest))))))))))

(define assoc_keys (lambda (l)
  (match l
    ((Nil) `(Nil))
    ((Cons p rest) (match p
      ((Pair k _) `(Cons ,k ,(assoc_keys rest))))))))

(define assoc_values (lambda (l)
  (match l
    ((Nil) `(Nil))
    ((Cons p rest) (match p
      ((Pair _ v) `(Cons ,v ,(assoc_values rest))))))))

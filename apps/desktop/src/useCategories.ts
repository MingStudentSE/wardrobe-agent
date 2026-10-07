import {useEffect,useState} from 'react';
import {api} from './api';
export const defaultCategories:Record<string,string>={top:'上装',bottom:'下装',outerwear:'外套',dress:'连衣裙',one_piece:'连体',shoes:'鞋',bag:'包',accessory:'配饰',underwear:'内衣',sportswear:'运动服',other:'其他'};
export type Category={id:string;name:string;count:number};
export function useCategories(member:string){const[labels,setLabels]=useState(defaultCategories);useEffect(()=>{let active=true;const load=()=>{void api<{items:Category[]}>('category.list',{member_id:member}).then(r=>{if(active)setLabels(Object.fromEntries(r.items.map(c=>[c.id,c.name])));}).catch(()=>{});};setLabels(defaultCategories);if(member)load();window.addEventListener('wardrobe-categories-changed',load);return()=>{active=false;window.removeEventListener('wardrobe-categories-changed',load);};},[member]);return labels;}
